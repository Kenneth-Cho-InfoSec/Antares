/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
package com.krystelligence.antares.engine

import android.app.Service
import android.content.Intent
import android.hardware.display.DisplayManager
import android.os.Binder
import android.os.Bundle
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.Build
import android.os.ParcelFileDescriptor
import android.os.Process
import android.os.RemoteException
import android.util.Log
import android.view.SurfaceControlViewHost
import android.view.MotionEvent
import android.view.inputmethod.InputMethodManager
import android.window.InputTransferToken
import com.krystelligence.antares.protocol.IAntaresEngine
import com.krystelligence.antares.protocol.IAntaresSession
import com.krystelligence.antares.protocol.IAntaresSessionCallback
import java.util.Collections
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import java.io.File
import org.servo.servoview.Servo
import org.servo.servoview.ServoView

/**
 * Hosts Servo-powered views in the Antares process and returns interactive surface packages to
 * Solipsism. Browser chrome, permissions, and user-facing policy remain owned by Solipsism.
 */
class AntaresEngineService : Service() {
    private val mainHandler = Handler(Looper.getMainLooper())
    private val sessions = Collections.synchronizedSet(mutableSetOf<AntaresSession>())
    private var renderHost: SurfaceControlViewHost? = null
    private var renderer: ServoView? = null
    private var activeSession: AntaresSession? = null
    private var blockAds = false
    private var blockGifs = false
    private var contentBlockingPolicy = ""
    private val mediaBridgeInstaller = Runnable {
        Log.d(TAG, "Installing HTML media bridge")
        renderer?.evaluateJavascript(AntaresMediaBridge.INSTALL_SCRIPT)
    }

    /** Servo's Android wrapper currently supports one native instance per process. */
    private val rendererClient = object : Servo.Client {
        override fun onAlert(message: String) = activeSession?.onAlert(message) ?: Unit
        override fun onLoadStarted() = activeSession?.onLoadStarted() ?: Unit
        override fun onLoadEnded() {
            scheduleMediaBridgeInstall()
            activeSession?.onLoadEnded()
        }
        override fun onTitleChanged(title: String) {
            val coordinateProbe = AntaresCoordinateProbe.decodeTitle(title)
            val mediaRequest = if (coordinateProbe == null) AntaresMediaBridge.decodeTitle(title) else null
            if (coordinateProbe != null) {
                activeSession?.onElementProbeResult(
                    coordinateProbe.requestId,
                    coordinateProbe.descriptor,
                )
            } else if (mediaRequest != null) {
                Log.d(
                    TAG,
                        "Forwarding native media request (source=true, cookies=${mediaRequest.cookies?.length ?: 0})",
                )
                activeSession?.openMedia(mediaRequest)
            } else {
                activeSession?.onTitleChanged(title)
            }
        }
        override fun onUrlChanged(url: String) {
            activeSession?.onUrlChanged(url)
            scheduleMediaBridgeInstall()
        }
        override fun onHistoryChanged(canGoBack: Boolean, canGoForward: Boolean) =
            activeSession?.onHistoryChanged(canGoBack, canGoForward) ?: Unit
        override fun onImeShow() = activeSession?.onImeShow() ?: Unit
        override fun onImeHide() = activeSession?.onImeHide() ?: Unit
        override fun onMediaSessionMetadata(title: String, artist: String, album: String) =
            activeSession?.onMediaSessionMetadata(title, artist, album) ?: Unit
        override fun onMediaSessionPlaybackStateChange(state: Int) =
            activeSession?.onMediaSessionPlaybackStateChange(state) ?: Unit
        override fun onMediaSessionSetPositionState(
            duration: Float,
            position: Float,
            playbackRate: Float,
        ) = activeSession?.onMediaSessionSetPositionState(duration, position, playbackRate) ?: Unit
    }

    private val engine = object : IAntaresEngine.Stub() {
        override fun getProtocolVersion(): Int {
            enforceClient()
            return AntaresProtocol.VERSION
        }

        override fun getEngineVersion(): String {
            enforceClient()
            return "Antares 0.1 (Servo)"
        }

        override fun getCapabilities(): Bundle {
            enforceClient()
            return Bundle().apply {
                putInt("protocol_version", AntaresProtocol.VERSION)
                putStringArray(AntaresProtocol.KEY_CAPABILITIES, AntaresProtocol.capabilities)
            }
        }

        override fun createSession(
            configuration: Bundle,
            callback: IAntaresSessionCallback,
        ): IAntaresSession {
            enforceClient()
            return AntaresSession(
                initialUrl = configuration.getString(AntaresProtocol.KEY_INITIAL_URL)
                    ?.takeIf(String::isNotBlank)
                    ?: "about:blank",
                experimental = configuration.getBoolean(AntaresProtocol.KEY_EXPERIMENTAL, false),
                userAgent = configuration.getString(AntaresProtocol.KEY_USER_AGENT).orEmpty(),
                callback = callback,
                onClosed = ::onSessionClosed,
            ).also(sessions::add)
        }
    }

    override fun onBind(intent: Intent?): IBinder = engine

    override fun onDestroy() {
        mainHandler.removeCallbacks(mediaBridgeInstaller)
        sessions.toList().forEach(AntaresSession::close)
        sessions.clear()
        releaseRenderer()
        super.onDestroy()
        // This service owns the entire :engine process. Servo's JNI runtime is single-use, so a
        // later binding must start in a fresh process rather than reinitialising it in this one.
        mainHandler.post { Process.killProcess(Process.myPid()) }
    }

    /**
     * Servo supports one native renderer per process. Release that renderer when the last logical
     * session closes so a later WebView -> Antares switch starts with a fresh surface hierarchy.
     */
    private fun onSessionClosed(session: AntaresSession) {
        sessions.remove(session)
    }

    private fun releaseRenderer() {
        activeSession = null
        renderer?.stop()
        renderer = null
        renderHost?.release()
        renderHost = null
    }

    private fun enforceClient() {
        val packages = packageManager.getPackagesForUid(Binder.getCallingUid()).orEmpty()
        if (AntaresProtocol.CLIENT_PACKAGE !in packages) {
            throw SecurityException("Antares Engine only accepts Solipsism Browser clients")
        }
    }

    private fun scheduleMediaBridgeInstall() {
        mainHandler.removeCallbacks(mediaBridgeInstaller)
        mainHandler.postDelayed(mediaBridgeInstaller, MEDIA_BRIDGE_INSTALL_DELAY_MS)
    }

    private inner class AntaresSession(
        private var initialUrl: String,
        private val experimental: Boolean,
        private var userAgent: String,
        private val callback: IAntaresSessionCallback,
        private val onClosed: (AntaresSession) -> Unit,
    ) : IAntaresSession.Stub(), Servo.Client {
        private var closed = false
        private var inputEnabled = true

        override fun attachSurface(
            displayId: Int,
            hostConfiguration: Bundle,
            width: Int,
            height: Int,
        ): Bundle = onMainBlocking {
            if (closed) return@onMainBlocking errorBundle("Session is closed")
            val display = getSystemService(DisplayManager::class.java).getDisplay(displayId)
                ?: return@onMainBlocking errorBundle("Display $displayId is unavailable")
            val switchingLogicalSession = activeSession !== this
            activeSession = this
            val shouldLoadInitialPage = renderer == null
            val currentRenderer = renderer
                ?: ServoView(this@AntaresEngineService).apply {
                    setClient(rendererClient)
                    setServoArgs(null, null, experimental)
                }
            renderHost?.release()
            val newHost = if (Build.VERSION.SDK_INT >= 35) {
                val inputToken = hostConfiguration.getParcelable(
                    AntaresProtocol.KEY_INPUT_TRANSFER_TOKEN,
                    InputTransferToken::class.java,
                ) ?: return@onMainBlocking errorBundle("Host input-transfer token is unavailable")
                SurfaceControlViewHost(this@AntaresEngineService, display, inputToken)
            } else {
                val hostToken = hostConfiguration.getBinder(AntaresProtocol.KEY_HOST_TOKEN)
                    ?: return@onMainBlocking errorBundle("Host surface token is unavailable")
                SurfaceControlViewHost(this@AntaresEngineService, display, hostToken)
            }
            // Keep the embedded hierarchy on Android's standard SurfaceControlViewHost path.
            // It supplies both the compositor and the normal remote input connection.
            newHost.setView(currentRenderer, width.coerceAtLeast(1), height.coerceAtLeast(1))
            currentRenderer.setUserAgent(userAgent)
            currentRenderer.setContentBlocking(blockAds, blockGifs, contentBlockingPolicy)
            currentRenderer.isFocusable = inputEnabled
            currentRenderer.isFocusableInTouchMode = inputEnabled
            if (!inputEnabled) currentRenderer.clearFocus()
            renderHost = newHost
            renderer = currentRenderer
            // Antares currently owns one Servo renderer and multiplexes Solipsism's logical tabs
            // over it. Rehosting the same logical session after a surface/window recreation must
            // preserve its live document. Attaching a different logical session, however, must
            // navigate the shared renderer to that session's last known URL; otherwise the rail
            // and selected-tab state change while the previous tab remains visible and reload also
            // targets that stale document.
            if (shouldLoadInitialPage || switchingLogicalSession) {
                currentRenderer.loadUri(initialUrl)
            }
            safeCallback { callback.onReady() }
            Bundle().apply {
                putParcelable(
                    AntaresProtocol.KEY_SURFACE_PACKAGE,
                    newHost.surfacePackage,
                )
            }
        }

        override fun surface(): Bundle = onMainBlocking {
            val surfacePackage = renderHost?.surfacePackage
                ?: return@onMainBlocking errorBundle("Session surface is not attached")
            Bundle().apply {
                putParcelable(AntaresProtocol.KEY_SURFACE_PACKAGE, surfacePackage)
            }
        }

        override fun resize(width: Int, height: Int) {
            mainHandler.post {
                if (activeSession === this) {
                    renderHost?.relayout(width.coerceAtLeast(1), height.coerceAtLeast(1))
                }
            }
        }

        override fun loadUrl(url: String?) {
            if (url.isNullOrBlank()) return
            initialUrl = url
            mainHandler.post { if (activeSession === this) renderer?.loadUri(url) }
        }

        override fun loadHtml(htmlFile: ParcelFileDescriptor?) {
            if (htmlFile == null || closed) return
            val pageUrl = runCatching {
                val pageDirectory = File(cacheDir, "offline-pages").also(File::mkdirs)
                val page = File(pageDirectory, "homepage-${System.nanoTime()}.html")
                ParcelFileDescriptor.AutoCloseInputStream(htmlFile).use { input ->
                    page.outputStream().use(input::copyTo)
                }
                page.toURI().toString()
            }.getOrElse { error ->
                safeCallback { callback.onAlert("Unable to load Solipsism's offline homepage: ${error.message}") }
                return
            }
            initialUrl = pageUrl
            mainHandler.post { if (activeSession === this) renderer?.loadUri(pageUrl) }
        }

        override fun goBack() = onActiveRenderer(ServoView::goBack)
        override fun goForward() = onActiveRenderer(ServoView::goForward)
        override fun reload() = onActiveRenderer(ServoView::reload)
        override fun stop() = onActiveRenderer(ServoView::stop)

        override fun setUserAgent(userAgent: String?) {
            this.userAgent = userAgent.orEmpty()
            mainHandler.post {
                if (activeSession === this) renderer?.setUserAgent(this.userAgent)
            }
        }

        override fun setContentBlocking(
            policyFile: ParcelFileDescriptor?,
            blockAds: Boolean,
            blockGifs: Boolean,
        ) {
            if (policyFile == null || closed) return
            val policy = ParcelFileDescriptor.AutoCloseInputStream(policyFile)
                .bufferedReader()
                .use { it.readText() }
            this@AntaresEngineService.blockAds = blockAds
            this@AntaresEngineService.blockGifs = blockGifs
            contentBlockingPolicy = policy
            onMainBlocking {
                renderer?.setContentBlocking(blockAds, blockGifs, policy)
            }
        }

        override fun setInputEnabled(enabled: Boolean): Boolean = onMainBlocking {
            if (closed) return@onMainBlocking false
            inputEnabled = enabled
            if (activeSession === this) {
                renderer?.apply {
                    isFocusable = enabled
                    isFocusableInTouchMode = enabled
                    // Re-enabling page input must not steal focus back from Solipsism chrome.
                    // ServoView requests focus itself when the user next touches the webpage.
                    if (!enabled) clearFocus()
                }
            }
            true
        }

        override fun dispatchTouchEvent(event: MotionEvent?) {
            if (event == null || closed) return
            val forwardedEvent = MotionEvent.obtain(event)
            mainHandler.post {
                try {
                    if (activeSession === this && inputEnabled) {
                        // This is an explicit cross-process input bridge, not Android's normal
                        // ViewGroup traversal. Invoke ServoView's terminal handler directly so
                        // no detached/windowless ViewRoot can reject the event before Servo sees
                        // it. This matches the endpoint used by ServoView for local WebView-like
                        // operation.
                        renderer?.onTouchEvent(forwardedEvent)
                    }
                } finally {
                    forwardedEvent.recycle()
                }
            }
        }

        override fun click(x: Float, y: Float) {
            mainHandler.post {
                if (activeSession === this && inputEnabled) renderer?.click(x, y)
            }
        }

        override fun scroll(dx: Int, dy: Int, x: Int, y: Int) {
            mainHandler.post {
                if (activeSession === this && inputEnabled) renderer?.scroll(dx, dy, x, y)
            }
        }

        override fun probeElement(requestId: Int, x: Float, y: Float) {
            mainHandler.post {
                if (activeSession === this && inputEnabled) {
                    renderer?.evaluateJavascript(AntaresCoordinateProbe.script(requestId, x, y))
                }
            }
        }

        override fun setForeground(foreground: Boolean) {
            mainHandler.post {
                if (activeSession === this) {
                    if (foreground) renderer?.onResume() else renderer?.onPause()
                }
            }
        }

        override fun close() {
            mainHandler.post {
                if (closed) return@post
                closed = true
                if (activeSession === this) activeSession = null
                onClosed(this)
            }
        }

        override fun onAlert(message: String) = safeCallback { callback.onAlert(message) }
        override fun onLoadStarted() = safeCallback { callback.onLoadStarted() }
        override fun onLoadEnded() = safeCallback { callback.onLoadEnded() }
        override fun onTitleChanged(title: String) = safeCallback { callback.onTitleChanged(title) }
        override fun onUrlChanged(url: String) {
            initialUrl = url
            safeCallback { callback.onUrlChanged(url) }
        }
        override fun onHistoryChanged(canGoBack: Boolean, canGoForward: Boolean) =
            safeCallback { callback.onHistoryChanged(canGoBack, canGoForward) }
        override fun onImeShow() {
            mainHandler.post {
                if (!inputEnabled || activeSession !== this) return@post
                getSystemService(InputMethodManager::class.java)?.showSoftInput(
                    renderer,
                    0,
                )
            }
        }
        override fun onImeHide() {
            mainHandler.post {
                // Clearing embedded focus is how Solipsism transfers the IME to its address bar.
                // Do not let the old remote client hide the newly requested host-window keyboard.
                if (!inputEnabled || activeSession !== this) return@post
                getSystemService(InputMethodManager::class.java)?.hideSoftInputFromWindow(
                    renderer?.windowToken,
                    0,
                )
            }
        }
        override fun onMediaSessionMetadata(title: String, artist: String, album: String) = Unit
        override fun onMediaSessionPlaybackStateChange(state: Int) = Unit
        override fun onMediaSessionSetPositionState(
            duration: Float,
            position: Float,
            playbackRate: Float,
        ) = Unit

        fun openMedia(request: AntaresMediaRequest) {
            if (closed || activeSession !== this) return
            safeCallback {
                callback.onMediaRequest(
                    Bundle().apply {
                        putString(AntaresProtocol.KEY_MEDIA_PAGE_URL, request.pageUrl)
                        putString(AntaresProtocol.KEY_MEDIA_DIRECT_SOURCE, request.directSource)
                        putString(AntaresProtocol.KEY_MEDIA_RENEWAL_REQUEST, request.renewalRequest)
                        putString(AntaresProtocol.KEY_MEDIA_COOKIES, request.cookies)
                        putString(AntaresProtocol.KEY_MEDIA_TITLE, request.title)
                    },
                )
            }
        }

        fun onElementProbeResult(requestId: Int, descriptor: String) {
            if (closed || activeSession !== this) return
            safeCallback { callback.onElementProbeResult(requestId, descriptor) }
        }

        private fun onActiveRenderer(action: (ServoView) -> Unit) {
            mainHandler.post {
                if (activeSession === this) renderer?.let(action)
            }
        }

        private fun safeCallback(block: () -> Unit) {
            try {
                block()
            } catch (_: RemoteException) {
                close()
            }
        }
    }

    private fun errorBundle(message: String) = Bundle().apply {
        putString(AntaresProtocol.KEY_ERROR, message)
    }

    private fun <T> onMainBlocking(block: () -> T): T {
        if (Looper.myLooper() == Looper.getMainLooper()) return block()
        val value = AtomicReference<T>()
        val failure = AtomicReference<Throwable>()
        val latch = CountDownLatch(1)
        mainHandler.post {
            try {
                value.set(block())
            } catch (error: Throwable) {
                failure.set(error)
            } finally {
                latch.countDown()
            }
        }
        check(latch.await(5, TimeUnit.SECONDS)) { "Timed out creating Antares surface" }
        failure.get()?.let { throw it }
        return value.get()
    }

    private companion object {
        const val TAG = "AntaresEngine"
        const val MEDIA_BRIDGE_INSTALL_DELAY_MS = 750L
    }

}

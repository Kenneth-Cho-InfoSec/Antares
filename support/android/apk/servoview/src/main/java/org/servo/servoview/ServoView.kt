/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
package org.servo.servoview

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.AttributeSet
import android.util.Log
import android.util.Size
import android.view.Choreographer
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import java.util.concurrent.atomic.AtomicBoolean

class ServoView : SurfaceView, Servo.RunCallback, Choreographer.FrameCallback {
    private val glThread: GLThread
    private val surfaceHolderCallback: SurfaceHolderCallback
    private var servo: Servo? = null
    private var servoArgs: String? = null
    private var initialUri: String? = null
    private var userAgent: String = ""
    private var darkTheme = false
    private var blockAds = false
    private var blockGifs = false
    private var contentBlockingPolicy = ""
    private var hostManagedInputMethod = false
    private val frameCallbackScheduled = AtomicBoolean(false)

    private var experimentalMode = false

    constructor(context: Context) : this(context, null)

    constructor(context: Context, attrs: AttributeSet?) : super(context, attrs) {
        isFocusable = true
        isFocusableInTouchMode = true
        isClickable = true
        addTouchables(arrayListOf(this))
        glThread = GLThread()
        surfaceHolderCallback = SurfaceHolderCallback(this)
        holder.addCallback(surfaceHolderCallback)
        glThread.start()
    }

    fun setClient(client: Servo.Client) {
        surfaceHolderCallback.client = client
    }

    /**
     * Leaves Android editor ownership with a separate embedding view while Servo continues to
     * receive web touch and key events through its explicit bridge.
     */
    fun setHostManagedInputMethod(enabled: Boolean) {
        hostManagedInputMethod = enabled
        if (enabled) clearFocus()
    }

    fun setServoArgs(args: String?, log: String?, experimentalMode: Boolean) {
        servoArgs = args
        surfaceHolderCallback.servoLog = log
        this.experimentalMode = experimentalMode
    }

    override fun inGLThread(f: Runnable) {
        glThread.glLooperHandler!!.post(f)
    }

    override fun inUIThread(f: Runnable) {
        post(f)
    }

    override fun requestVsync() {
        post {
            if (servo != null && frameCallbackScheduled.compareAndSet(false, true)) {
                Choreographer.getInstance().postFrameCallback(this)
            }
        }
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (event.keyCode != KeyEvent.KEYCODE_BACK) {
            servo!!.onKeyDown(keyCode, event)
            return true
        }
        return false
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (event.keyCode != KeyEvent.KEYCODE_BACK) {
            servo!!.onKeyUp(keyCode, event)
            return true
        }
        return false
    }

    fun commitText(text: String) {
        if (text.isNotEmpty()) servo?.imeInsertText(text)
    }

    fun sendKey(keyCode: Int) {
        val down = KeyEvent(KeyEvent.ACTION_DOWN, keyCode)
        val up = KeyEvent(KeyEvent.ACTION_UP, keyCode)
        servo?.onKeyDown(keyCode, down)
        servo?.onKeyUp(keyCode, up)
    }

    fun dismissIme() {
        servo?.imeDismissed()
    }

    override fun onTouchEvent(motionEvent: MotionEvent): Boolean {
        if (!hostManagedInputMethod) requestFocus()

        val action = motionEvent.actionMasked
        val pointerIndex = motionEvent.actionIndex
        val pointerId = motionEvent.getPointerId(pointerIndex)
        val x = motionEvent.getX(pointerIndex)
        val y = motionEvent.getY(pointerIndex)

        when (action) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> servo!!.touchDown(x, y, pointerId)
            MotionEvent.ACTION_MOVE -> servo!!.touchMove(x, y, pointerId)
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> servo!!.touchUp(x, y, pointerId)
            MotionEvent.ACTION_CANCEL -> servo!!.touchCancel(x, y, pointerId)
        }

        return true
    }

    override fun doFrame(frameTimeNanos: Long) {
        frameCallbackScheduled.set(false)
        servo?.onDoFrame(frameTimeNanos)
    }

    fun onPause() {
        Choreographer.getInstance().removeFrameCallback(this)
        frameCallbackScheduled.set(false)
        servo?.suspend(true)
    }

    fun onResume() {
        servo?.suspend(false)
    }

    fun reload() {
        servo!!.reload()
    }

    fun goBack() {
        servo!!.goBack()
    }

    fun goForward() {
        servo!!.goForward()
    }

    fun stop() {
        servo!!.stop()
    }

    fun click(x: Float, y: Float) {
        servo?.click(x, y)
    }

    fun scroll(dx: Int, dy: Int, x: Int, y: Int) {
        servo?.scroll(dx, dy, x, y)
    }

    fun loadUri(uri: String) {
        val servo = servo
        if (servo != null) {
            servo.loadUri(uri)
        } else {
            initialUri = uri
        }
    }

    fun evaluateJavascript(script: String) {
        servo?.evaluateJavascript(script)
    }

    fun setUserAgent(value: String) {
        userAgent = value
        servo?.setUserAgent(value)
    }

    fun setTheme(dark: Boolean) {
        darkTheme = dark
        servo?.setTheme(dark)
    }

    fun setContentBlocking(blockAds: Boolean, blockGifs: Boolean, policy: String) {
        this.blockAds = blockAds
        this.blockGifs = blockGifs
        contentBlockingPolicy = policy
        servo?.setContentBlocking(blockAds, blockGifs, policy)
    }

    fun mediaSessionAction(action: Int) {
        servo!!.mediaSessionAction(action)
    }

    fun setExperimentalMode(enable: Boolean) {
        servo?.setExperimentalMode(enable)
    }

    private class GLThread : Thread() {
        var glLooperHandler: Handler? = null

        override fun run() {
            Looper.prepare()

            glLooperHandler = Handler(Looper.myLooper()!!)

            Looper.loop()
        }
    }

    private class SurfaceHolderCallback(private val servoView: ServoView) : SurfaceHolder.Callback {
        var client: Servo.Client? = null
        var servoLog: String? = null
        private var paused = false

        override fun surfaceCreated(holder: SurfaceHolder) {
            Log.d(LOGTAG, "GLThread::surfaceCreated")

            val size = Size(servoView.width, servoView.height)

            val surface = holder.surface

            if (servoView.servo == null && !paused) {
                servoView.servo = Servo(
                    servoView.servoArgs,
                    servoView.initialUri,
                    size,
                    servoView.resources.displayMetrics.density,
                    servoLog,
                    true,
                    servoView.experimentalMode,
                    servoView.userAgent,
                    servoView.darkTheme,
                    servoView.blockAds,
                    servoView.blockGifs,
                    servoView.contentBlockingPolicy,
                    servoView,
                    client!!,
                    servoView.context,
                    surface,
                )
            } else {
                paused = false
                servoView.servo!!.resumePainting(surface, size)
            }

            servoView.requestVsync()
        }

        override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
            Log.d(LOGTAG, "GLThread::surfaceChanged")
            servoView.servo!!.resize(Size(width, height))
        }

        override fun surfaceDestroyed(holder: SurfaceHolder) {
            Log.d(LOGTAG, "GLThread::surfaceDestroyed")
            paused = true
            Choreographer.getInstance().removeFrameCallback(servoView)
            servoView.frameCallbackScheduled.set(false)
            servoView.servo!!.pausePainting()
        }
    }

    private companion object {
        private const val LOGTAG = "ServoView"
    }
}

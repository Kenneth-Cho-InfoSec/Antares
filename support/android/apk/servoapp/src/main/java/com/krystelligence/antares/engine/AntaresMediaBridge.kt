/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
package com.krystelligence.antares.engine

import android.net.Uri
import org.json.JSONObject

data class AntaresMediaRequest(
    val pageUrl: String,
    val directSource: String?,
    val renewalRequest: String?,
    val cookies: String?,
    val title: String?,
)

/**
 * A narrow bridge between Servo documents and Android's platform media decoder.
 *
 * Servo's Android cross-build currently uses its dummy media backend. This bridge keeps normal
 * page controls usable by forwarding a user-initiated HTML video request to a native player. It
 * does not expose an Android JavaScript object or accept arbitrary commands from page script.
 */
object AntaresMediaBridge {
    private const val TITLE_PREFIX = "__ANTARES_MEDIA_V1__:"

    val INSTALL_SCRIPT: String =
        """
        (() => {
          if (window.__antaresMediaBridgeInstalled) return;
          window.__antaresMediaBridgeInstalled = true;

          const findVideo = target => {
            if (!target || typeof target.closest !== 'function') return null;
            if (typeof target.matches === 'function' && target.matches('video')) return target;
            const container = target.closest('.video-js, .vjscontainer');
            return container ?
              (container.matches('video') ? container : container.querySelector('video')) :
              target.closest('video');
          };

          const mediaSources = [];
          const renewalRequests = [];
          const renewalCandidates = [];
          const pendingVideos = new WeakSet();
          let lastRequestAt = 0;

          const normaliseNetworkUrl = value => {
            if (typeof value !== 'string') return '';
            if (value.indexOf('//') === 0) return location.protocol + value;
            return /^https?:/i.test(value) ? value : '';
          };
          const isNetworkUrl = value => normaliseNetworkUrl(value) !== '';
          const rememberMediaSource = value => {
            const url = normaliseNetworkUrl(value);
            if (!url) return;
            const existing = mediaSources.findIndex(entry => entry.url === url);
            if (existing >= 0) mediaSources.splice(existing, 1);
            mediaSources.push({ url, observedAt: Date.now() });
            if (mediaSources.length > 12) mediaSources.shift();
          };
          const rememberRenewalRequest = (url, method, body, contentType) => {
            const target = normaliseNetworkUrl(url);
            const requestMethod = String(method || 'GET').toUpperCase();
            if (!target || (requestMethod !== 'GET' && requestMethod !== 'POST')) return;
            const requestBody = typeof body === 'string' ? body : '';
            if (requestBody.length > 32 * 1024) return;
            renewalRequests.push({
              url: target,
              method: requestMethod,
              body: requestBody,
              contentType: typeof contentType === 'string' ? contentType : '',
              observedAt: Date.now()
            });
            if (renewalRequests.length > 12) renewalRequests.shift();
          };
          const looksLikeMediaSource = value =>
            /\.(?:m3u8|mpd|mp4|m4v|webm|mov|ts)(?:[?#]|${'$'})/i.test(value);
          const rememberRenewalCandidate = (request, sources) => {
            if (!request || !sources || !sources.length) return;
            const normalisedSources = sources.map(normaliseNetworkUrl).filter(Boolean);
            if (!normalisedSources.length) return;
            renewalCandidates.push({ request, sources: normalisedSources, observedAt: Date.now() });
            if (renewalCandidates.length > 12) renewalCandidates.shift();
            if (normalisedSources.some(looksLikeMediaSource)) {
              rememberRenewalRequest(
                request.url,
                request.method,
                request.body,
                request.contentType
              );
            }
          };
          const associateRenewalWithSource = value => {
            const source = normaliseNetworkUrl(value);
            if (!source) return;
            for (let index = renewalCandidates.length - 1; index >= 0; index -= 1) {
              const candidate = renewalCandidates[index];
              if (!candidate.sources.includes(source)) continue;
              rememberRenewalRequest(
                candidate.request.url,
                candidate.request.method,
                candidate.request.body,
                candidate.request.contentType
              );
              return;
            }
          };
          const inspectPayload = payload => {
            const discovered = [];
            const visit = (value, key, depth) => {
              if (depth > 6 || value == null) return;
              if (typeof value === 'string') {
                // Page APIs do not use a common field name for a signed stream. Capture every
                // network URL from a response and only consider those observed after this video
                // was pressed, rather than making assumptions about a particular service schema.
                if (isNetworkUrl(value)) {
                  rememberMediaSource(value);
                  discovered.push(normaliseNetworkUrl(value));
                }
                return;
              }
              if (Array.isArray(value)) {
                value.slice(0, 32).forEach(item => visit(item, key, depth + 1));
                return;
              }
              if (typeof value === 'object') {
                Object.keys(value).slice(0, 64).forEach(name => visit(value[name], name, depth + 1));
              }
            };
            visit(payload, '', 0);
            return discovered;
          };
          const inspectResponseText = text => {
            if (!text || text.length > 512 * 1024) return [];
            try { return inspectPayload(JSON.parse(text)); } catch (_) {
              const urls = text.match(/https?:[^\s"'<>\\]+/g) || [];
              urls.forEach(rememberMediaSource);
              return urls.map(normaliseNetworkUrl).filter(Boolean);
            }
          };
          const observeFetch = () => {
            if (typeof window.fetch !== 'function') return;
            const originalFetch = window.fetch.bind(window);
            window.fetch = function() {
              const input = arguments[0];
              const options = arguments[1] || {};
              const request = {
                url: typeof input === 'string' ? input : input && input.url,
                method: options.method || (input && input.method) || 'GET',
                body: options.body,
                contentType: options.headers &&
                  (options.headers['Content-Type'] || options.headers['content-type'])
              };
              return originalFetch.apply(null, arguments).then(response => {
                try {
                  response.clone().text().then(text => {
                    rememberRenewalCandidate(request, inspectResponseText(text));
                  }).catch(() => {});
                } catch (_) {}
                return response;
              });
            };
          };
          const observeXhr = () => {
            if (!window.XMLHttpRequest || !XMLHttpRequest.prototype.send) return;
            const originalOpen = XMLHttpRequest.prototype.open;
            const originalSetRequestHeader = XMLHttpRequest.prototype.setRequestHeader;
            const originalSend = XMLHttpRequest.prototype.send;
            XMLHttpRequest.prototype.open = function(method, url) {
              this.__antaresMediaRequest = { method, url, contentType: '' };
              return originalOpen.apply(this, arguments);
            };
            XMLHttpRequest.prototype.setRequestHeader = function(name, value) {
              if (this.__antaresMediaRequest && String(name).toLowerCase() === 'content-type') {
                this.__antaresMediaRequest.contentType = String(value);
              }
              return originalSetRequestHeader.apply(this, arguments);
            };
            XMLHttpRequest.prototype.send = function() {
              if (this.__antaresMediaRequest) {
                this.__antaresMediaRequest.body =
                  typeof arguments[0] === 'string' ? arguments[0] : '';
              }
              this.addEventListener('load', () => {
                try {
                  if (typeof this.responseText === 'string') {
                    rememberRenewalCandidate(
                      this.__antaresMediaRequest,
                      inspectResponseText(this.responseText)
                    );
                  }
                } catch (_) {}
              });
              return originalSend.apply(this, arguments);
            };
          };
          const observeJQuery = () => {
            const install = () => {
              const jquery = window.jQuery;
              if (!jquery || !jquery.ajax || jquery.__antaresMediaObserved) return;
              const originalAjax = jquery.ajax;
              jquery.ajax = function() {
                const request = originalAjax.apply(this, arguments);
                try {
                  if (request && typeof request.done === 'function') {
                    request.done(payload => inspectPayload(payload));
                  }
                } catch (_) {}
                return request;
              };
              jquery.__antaresMediaObserved = true;
            };
            install();
            setTimeout(install, 1000);
          };
          const observeMediaSourceAssignment = () => {
            if (!window.HTMLMediaElement) return;
            const descriptor = Object.getOwnPropertyDescriptor(HTMLMediaElement.prototype, 'src');
            if (!descriptor || !descriptor.set || !descriptor.get) return;
            try {
              Object.defineProperty(HTMLMediaElement.prototype, 'src', {
                configurable: true,
                enumerable: descriptor.enumerable,
                get: descriptor.get,
                set: function(value) {
                  rememberMediaSource(value);
                  associateRenewalWithSource(value);
                  return descriptor.set.call(this, value);
                }
              });
            } catch (_) {}
          };
          observeFetch();
          observeXhr();
          observeJQuery();
          observeMediaSourceAssignment();

          const sourceFor = (video, notBefore) => {
            for (let index = mediaSources.length - 1; index >= 0; index -= 1) {
              if (mediaSources[index].observedAt >= notBefore) return mediaSources[index].url;
            }
            return '';
          };
          const directSourceFor = video => normaliseNetworkUrl(
            video.currentSrc || video.src ||
              (video.querySelector('source') && video.querySelector('source').src) || ''
          );
          const renewalFor = notBefore => {
            for (let index = renewalRequests.length - 1; index >= 0; index -= 1) {
              if (renewalRequests[index].observedAt >= notBefore) return renewalRequests[index];
            }
            return null;
          };
          const publishMediaRequest = (video, source, renewal) => {
            if (!source && !renewal) return false;
            const now = Date.now();
            if (now - lastRequestAt < 750) return true;
            lastRequestAt = now;
            const request = {
              requestId: now,
              pageUrl: location.href,
              directSource: source,
              renewalRequest: renewal ? JSON.stringify(renewal) : '',
              cookies: document.cookie || '',
              title: window.__antaresOriginalTitle || document.title || ''
            };
            // A JavaScript alert is modal and can leave Servo's document event loop blocked while
            // Android owns the foreground. Title notifications are asynchronous in the embedder,
            // so use a short-lived title signal and immediately restore the page's real title.
            const originalTitle = window.__antaresOriginalTitle || document.title;
            window.__antaresOriginalTitle = originalTitle;
            document.title = '$TITLE_PREFIX' + encodeURIComponent(JSON.stringify(request));
            // Servo title notifications cross an asynchronous JNI/UI boundary. Restoring the
            // title in the same event-loop turn can be coalesced away, especially after the
            // single renderer is re-hosted for a different tab. Keep the opaque marker long
            // enough for Android to consume it; the embedder filters it from browser chrome.
            setTimeout(() => {
              if (document.title.indexOf('$TITLE_PREFIX') === 0) document.title = originalTitle;
            }, 500);
            return true;
          };
          const deferUntilSourceIsReady = video => {
            if (pendingVideos.has(video)) return;
            pendingVideos.add(video);
            const pressedAt = Date.now();
            const sourceBeforePress = directSourceFor(video);
            let checksRemaining = 24;
            const check = () => {
              const currentSource = directSourceFor(video);
              const changedSource = currentSource && currentSource !== sourceBeforePress ?
                currentSource : '';
              const source = changedSource || sourceFor(video, pressedAt);
              const renewal = renewalFor(pressedAt);
              if (source || renewal) {
                pendingVideos.delete(video);
                publishMediaRequest(video, source, renewal);
              } else if (checksRemaining === 20) {
                // A media element may clear currentSrc after its built-in decoder rejects the
                // format. Replaying its most recently observed acquisition request gives the
                // native decoder a fresh signed URL. Static media instead falls back to the last
                // source after the page has had one second to replace it.
                const reusableRenewal = renewalFor(0);
                const reusableSource = sourceBeforePress || sourceFor(video, 0);
                pendingVideos.delete(video);
                publishMediaRequest(video, reusableSource, reusableRenewal);
              } else if (--checksRemaining > 0) {
                setTimeout(check, 250);
              } else {
                pendingVideos.delete(video);
              }
            };
            // Leave the page's gesture untouched so its player can renew expiring media URLs.
            // The native fallback opens only after a source or renewal request from this gesture
            // has been observed.
            setTimeout(check, 100);
          };
          const requestMedia = event => {
            const video = findVideo(event.target);
            if (!video) return;
            deferUntilSourceIsReady(video);
          };

          // Video.js and similar players often begin their own play pipeline on the press rather
          // than the final click. Capture the earliest practical gesture so Servo does not reach
          // its dummy media backend before Android's native Media3 fallback is opened.
          document.addEventListener('pointerdown', requestMedia, true);
          document.addEventListener('touchstart', requestMedia, true);
          document.addEventListener('mousedown', requestMedia, true);
          document.addEventListener('touchend', requestMedia, true);
          document.addEventListener('pointerup', requestMedia, true);
          document.addEventListener('click', requestMedia, true);
        })();
        """.trimIndent()

    fun decodeTitle(title: String): AntaresMediaRequest? {
        if (!title.startsWith(TITLE_PREFIX)) return null
        return runCatching {
            val json = JSONObject(Uri.decode(title.removePrefix(TITLE_PREFIX)))
            val pageUrl = json.optString("pageUrl").takeIf(String::isNotBlank)
                ?: return null
            AntaresMediaRequest(
                pageUrl = pageUrl,
                directSource = json.optString("directSource").takeIf(String::isNotBlank),
                renewalRequest = json.optString("renewalRequest").takeIf(String::isNotBlank),
                cookies = json.optString("cookies").takeIf(String::isNotBlank),
                title = json.optString("title").takeIf(String::isNotBlank),
            ).takeIf { it.directSource != null || it.renewalRequest != null }
        }.getOrNull()
    }
}

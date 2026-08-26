package dev.screengoated.toolbox.mobile.service.dj

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.webkit.JavascriptInterface
import android.webkit.WebView

/**
 * JavaScript bridge injected into the DJ WebView.
 * Called from JS when playback state changes, so we can update
 * the native MediaSession / notification.
 */
class DjJsBridge(private val context: Context) {
    private val mainHandler = Handler(Looper.getMainLooper())
    private var serviceStarted = false

    /**
     * Called from JS: liveMusicHelper playback-state-changed event.
     * States: "playing", "loading", "paused", "stopped"
     *
     * Every active update is delivered through the service intent, so the
     * first state cannot race service creation. Only explicit "stopped" ends
     * the service; pauses and loading stalls keep it alive.
     */
    @JavascriptInterface
    fun onPlaybackStateChanged(state: String) {
        mainHandler.post {
            val wantsActive = state == "playing" || state == "loading" || state == "paused"
            DjWebViewHolder.updatePlaybackState(state == "playing" || state == "loading")

            if (wantsActive) {
                serviceStarted = DjPlaybackService.update(context, state)
            }

            // Only kill the service on explicit stop — never on pause/buffering.
            if (state == "stopped" && serviceStarted) {
                serviceStarted = false
                DjPlaybackService.stop(context)
            }
        }
    }

    /** Called from JS: sets the currently playing title for notification metadata. */
    @JavascriptInterface
    fun onTitleChanged(title: String) {
        mainHandler.post {
            val currentState = if (DjWebViewHolder.isPlaying) "playing" else "paused"
            if (serviceStarted) {
                DjPlaybackService.update(context, currentState, title)
            }
        }
    }

    /** Wire platform media-session controls to the WebView. */
    fun wireNotificationCallbacks(webView: WebView) {
        DjWebViewHolder.onPlayFromNotification = {
            mainHandler.post {
                webView.evaluateJavascript(
                    "window.postMessage({ type: 'pm-dj-play' }, '*')",
                    null
                )
            }
        }
        DjWebViewHolder.onPauseFromNotification = {
            mainHandler.post {
                webView.evaluateJavascript(
                    "window.postMessage({ type: 'pm-dj-pause' }, '*')",
                    null
                )
            }
        }
        DjWebViewHolder.onStopFromNotification = {
            mainHandler.post {
                webView.evaluateJavascript(
                    "window.postMessage({ type: 'pm-dj-stop-audio' }, '*')",
                    null
                )
            }
        }
    }
}

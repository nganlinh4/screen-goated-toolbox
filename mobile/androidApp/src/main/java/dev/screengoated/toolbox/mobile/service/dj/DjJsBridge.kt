package dev.screengoated.toolbox.mobile.service.dj

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.media3.common.util.UnstableApi

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
     * Only starts the service on first play, and only stops it on
     * explicit "stopped". Pauses and loading stalls keep the service alive.
     */
    @JavascriptInterface
    fun onPlaybackStateChanged(state: String) {
        mainHandler.post {
            val wantsActive = state == "playing" || state == "loading" || state == "paused"
            DjWebViewHolder.updatePlaybackState(state == "playing" || state == "loading")

            // Start service on first active state
            if (wantsActive && !serviceStarted) {
                serviceStarted = true
                DjPlaybackService.start(context)
            }

            // Forward state to the virtual player (updates notification)
            @OptIn(UnstableApi::class)
            DjPlaybackService.playerRef?.updateFromWebView(state, null)

            // Only kill service on explicit stop — never on pause/buffering
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
            @OptIn(UnstableApi::class)
            DjPlaybackService.playerRef?.updateFromWebView(currentState, title)
        }
    }

    /** Wire notification controls -> WebView JS. */
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

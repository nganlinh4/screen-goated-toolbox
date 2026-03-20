package dev.screengoated.toolbox.mobile.service.dj

import android.webkit.WebView

/**
 * Singleton that keeps the DJ WebView alive across navigation.
 * When the user leaves DjScreen, the WebView is detached from the view
 * hierarchy but NOT destroyed, so AudioContext keeps playing.
 */
object DjWebViewHolder {
    var webView: WebView? = null
        private set

    var isPlaying: Boolean = false
        private set

    /** Callbacks from notification controls → WebView. */
    var onPlayFromNotification: (() -> Unit)? = null
    var onPauseFromNotification: (() -> Unit)? = null
    var onStopFromNotification: (() -> Unit)? = null

    fun attach(wv: WebView) {
        webView = wv
    }

    fun updatePlaybackState(playing: Boolean) {
        isPlaying = playing
    }

    fun destroy() {
        onPlayFromNotification = null
        onPauseFromNotification = null
        onStopFromNotification = null
        webView?.stopLoading()
        webView?.destroy()
        webView = null
        isPlaying = false
    }
}

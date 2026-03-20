package dev.screengoated.toolbox.mobile.service.dj

import android.os.Looper
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture

/**
 * A virtual [Player] that doesn't decode audio itself.
 * It mirrors the playback state reported by the DJ WebView via JS bridge,
 * and forwards notification control actions (play/pause) back to the WebView.
 *
 * State mapping from WebView → Player:
 *   "playing"  → STATE_READY,     playWhenReady=true
 *   "loading"  → STATE_BUFFERING, playWhenReady=true
 *   "paused"   → STATE_READY,     playWhenReady=false  (keeps service alive)
 *   "stopped"  → STATE_ENDED,     playWhenReady=false   (service can stop)
 */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
class DjWebViewPlayer(looper: Looper) : SimpleBasePlayer(looper) {

    private var playWhenReady = false
    private var playbackState = Player.STATE_IDLE
    private var currentTitle = "Be a DJ"

    override fun getState(): State {
        val metadata = MediaMetadata.Builder()
            .setTitle(currentTitle)
            .setArtist("Screen Goated Toolbox")
            .build()

        val mediaItem = MediaItem.Builder()
            .setMediaMetadata(metadata)
            .build()

        val mediaItemData = MediaItemData.Builder(/* uid = */ "dj-audio")
            .setMediaItem(mediaItem)
            .setMediaMetadata(metadata)
            .setIsPlaceholder(false)
            .build()

        return State.Builder()
            .setAvailableCommands(
                Player.Commands.Builder()
                    .add(Player.COMMAND_PLAY_PAUSE)
                    .add(Player.COMMAND_STOP)
                    .build()
            )
            .setPlayWhenReady(playWhenReady, PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
            .setPlaybackState(playbackState)
            .setPlaylist(listOf(mediaItemData))
            .setCurrentMediaItemIndex(0)
            .build()
    }

    override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
        this.playWhenReady = playWhenReady
        if (playWhenReady) {
            this.playbackState = Player.STATE_BUFFERING
            DjWebViewHolder.onPlayFromNotification?.invoke()
        } else {
            this.playbackState = Player.STATE_READY // paused, not idle
            DjWebViewHolder.onPauseFromNotification?.invoke()
        }
        return Futures.immediateVoidFuture()
    }

    override fun handleStop(): ListenableFuture<*> {
        playWhenReady = false
        playbackState = Player.STATE_ENDED
        DjWebViewHolder.onStopFromNotification?.invoke()
        return Futures.immediateVoidFuture()
    }

    /**
     * Called from the JS bridge when the WebView reports a state change.
     * Maps the WebView state string to appropriate Player state constants
     * to keep the foreground service alive during buffering/pauses.
     */
    fun updateFromWebView(webViewState: String, title: String?) {
        if (title != null) currentTitle = title

        when (webViewState) {
            "playing" -> {
                playWhenReady = true
                playbackState = Player.STATE_READY
            }
            "loading" -> {
                playWhenReady = true
                playbackState = Player.STATE_BUFFERING
            }
            "paused" -> {
                // Keep STATE_READY so Media3 doesn't tear down the service.
                // User can resume; temporary stalls won't kill playback.
                playWhenReady = false
                playbackState = Player.STATE_READY
            }
            "stopped" -> {
                playWhenReady = false
                playbackState = Player.STATE_ENDED
            }
        }
        invalidateState()
    }
}

package dev.screengoated.toolbox.mobile.service

import android.app.PendingIntent
import android.content.ComponentName
import android.content.Intent
import android.os.Build
import android.provider.Settings
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import android.util.Log

internal enum class BubbleTileAction {
    STOP,
    REQUEST_OVERLAY_PERMISSION,
    START,
}

internal fun bubbleTileAction(isRunning: Boolean, canDrawOverlays: Boolean): BubbleTileAction =
    when {
        isRunning -> BubbleTileAction.STOP
        !canDrawOverlays -> BubbleTileAction.REQUEST_OVERLAY_PERMISSION
        else -> BubbleTileAction.START
    }

class SgtTileService : TileService() {
    override fun onStartListening() {
        super.onStartListening()
        qsTile?.state = if (BubbleService.isRunning) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        qsTile?.updateTile()
    }

    override fun onClick() {
        super.onClick()
        when (bubbleTileAction(BubbleService.isRunning, Settings.canDrawOverlays(this))) {
            BubbleTileAction.STOP -> {
                Log.i(TAG, "tile_click action=stop")
                stopService(Intent(this, BubbleService::class.java))
                updateState(Tile.STATE_INACTIVE)
            }
            BubbleTileAction.REQUEST_OVERLAY_PERMISSION -> {
                Log.i(TAG, "tile_click action=request_overlay_permission")
                updateState(Tile.STATE_INACTIVE)
                launchPermissionFlow()
            }
            BubbleTileAction.START -> {
                Log.i(TAG, "tile_click action=start")
                val accepted = tryStartForegroundService(
                    this,
                    Intent(this, BubbleService::class.java),
                    TAG,
                )
                updateState(
                    if (accepted && BubbleService.isRunning) {
                        Tile.STATE_ACTIVE
                    } else {
                        Tile.STATE_INACTIVE
                    },
                )
            }
        }
    }

    private fun launchPermissionFlow() {
        val intent = Intent(this, BubblePermissionActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startActivityAndCollapse(
                PendingIntent.getActivity(
                    this,
                    PERMISSION_REQUEST_CODE,
                    intent,
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                ),
            )
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }

    private fun updateState(state: Int) {
        qsTile?.state = state
        qsTile?.updateTile()
    }

    companion object {
        private const val TAG = "SgtTileService"
        private const val PERMISSION_REQUEST_CODE = 4201

        fun requestStateRefresh(context: android.content.Context) {
            TileService.requestListeningState(
                context,
                ComponentName(context, SgtTileService::class.java),
            )
        }
    }
}

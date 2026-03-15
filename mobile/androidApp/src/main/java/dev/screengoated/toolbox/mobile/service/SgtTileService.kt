package dev.screengoated.toolbox.mobile.service

import android.content.Intent
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

class SgtTileService : TileService() {
    override fun onStartListening() {
        super.onStartListening()
        qsTile?.state = if (BubbleService.isRunning) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        qsTile?.updateTile()
    }

    override fun onClick() {
        super.onClick()
        if (BubbleService.isRunning) {
            stopService(Intent(this, BubbleService::class.java))
            qsTile?.state = Tile.STATE_INACTIVE
        } else {
            val intent = Intent(this, BubbleService::class.java)
            startForegroundService(intent)
            qsTile?.state = Tile.STATE_ACTIVE
        }
        qsTile?.updateTile()
    }
}

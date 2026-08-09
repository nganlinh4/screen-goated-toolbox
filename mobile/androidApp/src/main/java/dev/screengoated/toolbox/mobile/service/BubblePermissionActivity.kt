package dev.screengoated.toolbox.mobile.service

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import android.util.Log
import androidx.core.net.toUri

class BubblePermissionActivity : Activity() {
    private var permissionScreenOpened = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        permissionScreenOpened = savedInstanceState?.getBoolean(KEY_PERMISSION_OPENED) == true
    }

    override fun onSaveInstanceState(outState: Bundle) {
        outState.putBoolean(KEY_PERMISSION_OPENED, permissionScreenOpened)
        super.onSaveInstanceState(outState)
    }

    override fun onResume() {
        super.onResume()
        when {
            Settings.canDrawOverlays(this) -> {
                Log.i(TAG, "overlay_permission_result granted=true")
                tryStartForegroundService(
                    this,
                    Intent(this, BubbleService::class.java),
                    TAG,
                )
                finish()
            }
            permissionScreenOpened -> {
                Log.i(TAG, "overlay_permission_result granted=false")
                SgtTileService.requestStateRefresh(this)
                finish()
            }
            else -> {
                permissionScreenOpened = true
                runCatching {
                    startActivity(
                        Intent(
                            Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                            "package:$packageName".toUri(),
                        ),
                    )
                }.onFailure { error ->
                    Log.e(TAG, "Could not open overlay permission settings", error)
                    SgtTileService.requestStateRefresh(this)
                    finish()
                }
            }
        }
    }

    private companion object {
        const val TAG = "BubblePermission"
        const val KEY_PERMISSION_OPENED = "permission_screen_opened"
    }
}

package dev.screengoated.toolbox.mobile.phonecontrol.projection

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.media.projection.MediaProjectionConfig
import android.media.projection.MediaProjectionManager
import android.os.Build
import androidx.annotation.RequiresApi

internal class PhoneControlProjectionGrant private constructor(
    val resultCode: Int,
    val data: Intent,
) {
    companion object {
        fun fromActivityResult(resultCode: Int, data: Intent?): PhoneControlProjectionGrant? {
            if (resultCode != Activity.RESULT_OK || data == null) return null
            return PhoneControlProjectionGrant(resultCode, Intent(data))
        }
    }
}

internal fun createPhoneControlProjectionConsentIntent(context: Context): Intent? {
    val manager = context.getSystemService(MediaProjectionManager::class.java) ?: return null
    return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
        createDefaultDisplayConsentIntent(manager)
    } else {
        manager.createScreenCaptureIntent()
    }
}

@RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
private fun createDefaultDisplayConsentIntent(manager: MediaProjectionManager): Intent =
    manager.createScreenCaptureIntent(MediaProjectionConfig.createConfigForDefaultDisplay())

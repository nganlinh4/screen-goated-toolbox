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

internal fun Intent.phoneControlProjectionGrant(): PhoneControlProjectionGrant? {
    val resultCode = getIntExtra(PROJECTION_RESULT_CODE_EXTRA, Int.MIN_VALUE)
    val data = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        getParcelableExtra(PROJECTION_DATA_EXTRA, Intent::class.java)
    } else {
        @Suppress("DEPRECATION")
        getParcelableExtra(PROJECTION_DATA_EXTRA)
    }
    return PhoneControlProjectionGrant.fromActivityResult(resultCode, data)
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

internal const val PROJECTION_RESULT_CODE_EXTRA =
    "dev.screengoated.toolbox.mobile.phonecontrol.PROJECTION_RESULT_CODE"
internal const val PROJECTION_DATA_EXTRA =
    "dev.screengoated.toolbox.mobile.phonecontrol.PROJECTION_DATA"

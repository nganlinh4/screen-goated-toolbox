package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import dev.screengoated.toolbox.mobile.phonecontrol.projection.createPhoneControlProjectionConsentIntent

/** Consent-driven, debug-only surface for real projection acceptance. */
class ProjectionCaptureFixtureActivity : Activity() {
    lateinit var patch: View
        private set

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        instance = this
        patch = View(this).apply { setBackgroundColor(Color.BLUE) }
        setContentView(FrameLayout(this).apply {
            setBackgroundColor(Color.BLACK)
            addView(patch, FrameLayout.LayoutParams(96, 96, Gravity.CENTER))
            keepScreenOn = true
        })
        @Suppress("DEPRECATION")
        startActivityForResult(requireNotNull(createPhoneControlProjectionConsentIntent(this)), REQUEST)
    }

    @Deprecated("Activity result compatibility for the debug acceptance surface")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != REQUEST || resultCode != RESULT_OK || data == null) return
        startForegroundService(Intent(this, ProjectionCaptureFixtureService::class.java)
            .putExtra("resultCode", resultCode).putExtra("resultData", data))
    }

    override fun onDestroy() {
        instance = null
        stopService(Intent(this, ProjectionCaptureFixtureService::class.java))
        super.onDestroy()
    }

    companion object {
        private const val REQUEST = 2197
        @Volatile var instance: ProjectionCaptureFixtureActivity? = null
            private set
    }
}

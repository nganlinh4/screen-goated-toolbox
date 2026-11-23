package dev.screengoated.toolbox.mobile

import android.content.Context
import dev.screengoated.toolbox.mobile.ui.i18n.uiLocalized
import android.content.Intent
import android.media.projection.MediaProjectionManager
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import dev.screengoated.toolbox.mobile.service.LiveTranslateService

class ProjectionConsentProxyActivity : ComponentActivity() {
    private var startupBlocked = false
    private val projectionLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        val appContainer = (application as SgtMobileApplication).appContainerOrNull
        if (startupBlocked || appContainer == null) {
            finishProxy()
            return@registerForActivityResult
        }
        val flow = intent?.getStringExtra(EXTRA_FLOW)
        val requestId = intent?.getStringExtra(EXTRA_AUDIO_REQUEST_ID)
        if (flow == FLOW_RESUME_CAPTURE_PRESET &&
            (requestId == null || appContainer.audioPresetLaunchStore.peek()?.requestId != requestId)
        ) {
            finishProxy()
            return@registerForActivityResult
        }
        if (result.resultCode == RESULT_OK) {
            appContainer.repository.rememberProjectionConsent(result.resultCode, result.data)
            when (flow) {
                FLOW_RESUME_CAPTURE_PRESET -> appContainer.audioPresetLaunchStore.resumeIfOwned(requireNotNull(requestId))
                FLOW_START_SESSION -> LiveTranslateService.start(this)
            }
        } else {
            if (flow == FLOW_RESUME_CAPTURE_PRESET && requestId != null) {
                appContainer.audioPresetLaunchStore.clearIfId(requestId)
            }
            Toast.makeText(
                this,
                uiLocalized().getString(R.string.projection_audio_consent_cancelled),
                Toast.LENGTH_SHORT,
            ).show()
        }
        finishProxy()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if ((application as SgtMobileApplication).appContainerOrNull == null) {
            startupBlocked = true
            finishProxy()
            return
        }
        if (savedInstanceState == null) {
            launchProjectionConsent()
        }
    }

    private fun launchProjectionConsent() {
        val flow = intent?.getStringExtra(EXTRA_FLOW)
        val requestId = intent?.getStringExtra(EXTRA_AUDIO_REQUEST_ID)
        val container = (application as SgtMobileApplication).appContainerOrNull
        if (container == null) {
            finishProxy()
            return
        }
        val store = container.audioPresetLaunchStore
        if (flow == FLOW_RESUME_CAPTURE_PRESET &&
            (requestId == null || store.peek()?.requestId != requestId)
        ) {
            finishProxy()
            return
        }
        val projectionManager = getSystemService(MediaProjectionManager::class.java)
        if (projectionManager == null) {
            if (flow == FLOW_RESUME_CAPTURE_PRESET && requestId != null) {
                store.clearIfId(requestId)
            }
            Toast.makeText(this, uiLocalized().getString(R.string.projection_unavailable), Toast.LENGTH_SHORT).show()
            finishProxy()
            return
        }
        projectionLauncher.launch(projectionManager.createScreenCaptureIntent())
    }

    private fun finishProxy() {
        finish()
    }

    companion object {
        private const val EXTRA_FLOW = "dev.screengoated.toolbox.mobile.extra.PROJECTION_FLOW"
        private const val EXTRA_AUDIO_REQUEST_ID = "dev.screengoated.toolbox.mobile.extra.AUDIO_REQUEST_ID"
        private const val FLOW_START_SESSION = "start_session"
        private const val FLOW_RESUME_CAPTURE_PRESET = "resume_capture_preset"

        fun startSessionIntent(context: Context): Intent {
            return Intent(context, ProjectionConsentProxyActivity::class.java)
                .putExtra(EXTRA_FLOW, FLOW_START_SESSION)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
        }

        fun resumeCapturePresetIntent(context: Context, requestId: String): Intent {
            return Intent(context, ProjectionConsentProxyActivity::class.java)
                .putExtra(EXTRA_FLOW, FLOW_RESUME_CAPTURE_PRESET)
                .putExtra(EXTRA_AUDIO_REQUEST_ID, requestId)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
        }

    }
}

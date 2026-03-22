package dev.screengoated.toolbox.mobile

import android.content.Context
import android.content.Intent
import android.media.projection.MediaProjectionManager
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import dev.screengoated.toolbox.mobile.preset.AudioPresetLaunchKind
import dev.screengoated.toolbox.mobile.service.BubbleService
import dev.screengoated.toolbox.mobile.service.LiveTranslateService

class ProjectionConsentProxyActivity : ComponentActivity() {
    private val projectionLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        val appContainer = (application as SgtMobileApplication).appContainer
        if (result.resultCode == RESULT_OK) {
            appContainer.repository.rememberProjectionConsent(result.resultCode, result.data)
            when (intent?.getStringExtra(EXTRA_FLOW)) {
                FLOW_RESUME_CAPTURE_PRESET -> BubbleService.resumePendingAudioPreset(this)
                FLOW_RESUME_REALTIME_PRESET -> {
                    val pending = appContainer.audioPresetLaunchStore.peek()
                    if (pending?.kind != AudioPresetLaunchKind.REALTIME) {
                        appContainer.audioPresetLaunchStore.clear()
                    } else {
                        val resolved = appContainer.presetRepository.getResolvedPreset(pending.presetId)
                        if (resolved == null) {
                            appContainer.audioPresetLaunchStore.clear()
                            Toast.makeText(
                                this,
                                "The requested realtime audio preset is unavailable.",
                                Toast.LENGTH_SHORT,
                            ).show()
                        } else {
                            appContainer.repository.applyTransientSessionConfig(
                                resolved.preset.toRealtimeSessionConfig(
                                    fallback = appContainer.repository.currentConfig(),
                                ),
                            )
                            appContainer.audioPresetLaunchStore.setActiveRealtimePresetId(resolved.preset.id)
                            appContainer.audioPresetLaunchStore.clear()
                            LiveTranslateService.start(this)
                        }
                    }
                }
                else -> LiveTranslateService.start(this)
            }
        } else {
            if (intent?.getStringExtra(EXTRA_FLOW) == FLOW_RESUME_CAPTURE_PRESET ||
                intent?.getStringExtra(EXTRA_FLOW) == FLOW_RESUME_REALTIME_PRESET
            ) {
                appContainer.audioPresetLaunchStore.clear()
            }
            Toast.makeText(
                this,
                "Device audio capture consent was cancelled.",
                Toast.LENGTH_SHORT,
            ).show()
        }
        finishProxy()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (savedInstanceState == null) {
            launchProjectionConsent()
        }
    }

    private fun launchProjectionConsent() {
        val projectionManager = getSystemService(MediaProjectionManager::class.java)
        if (projectionManager == null) {
            Toast.makeText(this, "MediaProjection is unavailable on this device.", Toast.LENGTH_SHORT).show()
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
        private const val FLOW_START_SESSION = "start_session"
        private const val FLOW_RESUME_CAPTURE_PRESET = "resume_capture_preset"
        private const val FLOW_RESUME_REALTIME_PRESET = "resume_realtime_preset"

        fun startSessionIntent(context: Context): Intent {
            return Intent(context, ProjectionConsentProxyActivity::class.java)
                .putExtra(EXTRA_FLOW, FLOW_START_SESSION)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
        }

        fun resumeCapturePresetIntent(context: Context): Intent {
            return Intent(context, ProjectionConsentProxyActivity::class.java)
                .putExtra(EXTRA_FLOW, FLOW_RESUME_CAPTURE_PRESET)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
        }

        fun resumeRealtimePresetIntent(context: Context): Intent {
            return Intent(context, ProjectionConsentProxyActivity::class.java)
                .putExtra(EXTRA_FLOW, FLOW_RESUME_REALTIME_PRESET)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
        }
    }
}

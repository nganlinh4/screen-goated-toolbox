package dev.screengoated.toolbox.mobile

import android.Manifest
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import dev.screengoated.toolbox.mobile.preset.AudioPermissionDecision
import dev.screengoated.toolbox.mobile.preset.refreshedAudioPermissionDecision
import dev.screengoated.toolbox.mobile.ui.i18n.uiLocalized

/** App-private permission handoff for the capture selected by the user. */
class AudioPresetPermissionActivity : ComponentActivity() {
    private var startupBlocked = false
    private val requestId: String? get() = intent?.getStringExtra(EXTRA_REQUEST_ID)

    private val permissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        val id = requestId
        val appContainer = (application as SgtMobileApplication).appContainerOrNull
        if (startupBlocked || appContainer == null) {
            finish()
            return@registerForActivityResult
        }
        val store = appContainer.audioPresetLaunchStore
        when (refreshedAudioPermissionDecision(id, store, granted, appContainer.repository::refreshPermissions)) {
            AudioPermissionDecision.RESUME_CAPTURE -> store.resumeIfOwned(requireNotNull(id))
            AudioPermissionDecision.CLEAR_CAPTURE -> {
                store.clearIfId(requireNotNull(id))
                Toast.makeText(this, uiLocalized().getString(R.string.preset_audio_permission_required), Toast.LENGTH_SHORT).show()
            }
            AudioPermissionDecision.IGNORE -> Unit
        }
        finish()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val id = requestId
        val container = (application as SgtMobileApplication).appContainerOrNull
        if (container == null) {
            startupBlocked = true
            finish()
            return
        }
        val pending = container.audioPresetLaunchStore.peek()
        if (id == null || pending?.requestId != id) {
            finish()
        } else if (savedInstanceState == null) {
            permissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
        }
    }

    companion object {
        private const val EXTRA_REQUEST_ID = "dev.screengoated.toolbox.mobile.extra.AUDIO_REQUEST_ID"

        fun resumeCapturePresetIntent(context: Context, requestId: String): Intent =
            Intent(context, AudioPresetPermissionActivity::class.java)
                .putExtra(EXTRA_REQUEST_ID, requestId)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
}

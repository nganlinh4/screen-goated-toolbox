package dev.screengoated.toolbox.mobile

import android.content.Intent
import android.media.projection.MediaProjectionManager
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.runtime.rememberUpdatedState
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.screengoated.toolbox.mobile.shared.live.DisplayMode
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.ui.SgtMobileApp
import dev.screengoated.toolbox.mobile.ui.theme.SgtMobileTheme

class MainActivity : ComponentActivity() {
    private val viewModel: MainViewModel by viewModels {
        MainViewModel.factory(application as SgtMobileApplication)
    }

    private var pendingStart = false
    private var autoStartOnResume = false

    private val permissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        viewModel.refreshPermissions()
        if (pendingStart) {
            if (viewModel.sessionState.value.permissions.recordAudioGranted &&
                viewModel.sessionState.value.permissions.notificationsGranted
            ) {
                continueStartFlow()
            } else {
                pendingStart = false
                viewModel.fail("Live translate needs the requested runtime permissions.")
            }
        }
    }

    private val overlayLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) {
        viewModel.refreshPermissions()
        if (pendingStart) {
            if (viewModel.sessionState.value.permissions.overlayGranted) {
                continueStartFlow()
            } else {
                pendingStart = false
                viewModel.fail("Overlay permission is required for the floating live window.")
            }
        }
    }

    private val projectionLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        if (result.resultCode == RESULT_OK) {
            viewModel.rememberProjectionConsent(result.resultCode, result.data)
            if (pendingStart) {
                continueStartFlow()
            }
        } else {
            pendingStart = false
            viewModel.refreshPermissions()
            viewModel.fail("Device audio capture consent was cancelled.")
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        handleIntent(intent)
        enableEdgeToEdge()

        setContent {
            val state by viewModel.sessionState.collectAsStateWithLifecycle()
            val apiKey by viewModel.apiKey.collectAsStateWithLifecycle()
            val cerebrasApiKey by viewModel.cerebrasApiKey.collectAsStateWithLifecycle()
            val globalTtsSettings by viewModel.globalTtsSettings.collectAsStateWithLifecycle()
            val context = LocalContext.current
            val onSessionToggle by rememberUpdatedState {
                if (state.phase == dev.screengoated.toolbox.mobile.shared.live.SessionPhase.LISTENING ||
                    state.phase == dev.screengoated.toolbox.mobile.shared.live.SessionPhase.TRANSLATING ||
                    state.phase == dev.screengoated.toolbox.mobile.shared.live.SessionPhase.STARTING
                ) {
                    pendingStart = false
                    viewModel.stopSession(this@MainActivity)
                } else {
                    pendingStart = true
                    continueStartFlow()
                }
            }

            LaunchedEffect(state.lastError) {
                state.lastError?.takeIf { it.isNotBlank() }?.let { message ->
                    Toast.makeText(context, message, Toast.LENGTH_SHORT).show()
                }
            }

            SgtMobileTheme {
                SgtMobileApp(
                    state = state,
                    apiKey = apiKey,
                    cerebrasApiKey = cerebrasApiKey,
                    globalTtsSettings = globalTtsSettings,
                    onApiKeyChanged = viewModel::onApiKeyChanged,
                    onCerebrasApiKeyChanged = viewModel::onCerebrasApiKeyChanged,
                    onGlobalTtsMethodChanged = viewModel::onGlobalTtsMethodChanged,
                    onGlobalTtsSpeedPresetChanged = viewModel::onGlobalTtsSpeedPresetChanged,
                    onGlobalTtsVoiceChanged = viewModel::onGlobalTtsVoiceChanged,
                    onGlobalTtsConditionsChanged = viewModel::onGlobalTtsConditionsChanged,
                    onGlobalEdgeTtsSettingsChanged = viewModel::onGlobalEdgeTtsSettingsChanged,
                    onSessionToggle = onSessionToggle,
                )
            }
        }
    }

    override fun onResume() {
        super.onResume()
        viewModel.refreshPermissions()
        if (autoStartOnResume) {
            autoStartOnResume = false
            pendingStart = true
            continueStartFlow()
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleIntent(intent)
    }

    private fun continueStartFlow() {
        val state = viewModel.sessionState.value
        if (!viewModel.hasApiKey()) {
            pendingStart = false
            viewModel.fail("Enter your Gemini BYOK key before starting live translate.")
            return
        }

        val missingRuntimePermissions = buildList {
            if (!state.permissions.recordAudioGranted) {
                add(android.Manifest.permission.RECORD_AUDIO)
            }
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU &&
                !state.permissions.notificationsGranted
            ) {
                add(android.Manifest.permission.POST_NOTIFICATIONS)
            }
        }

        when {
            missingRuntimePermissions.isNotEmpty() -> {
                permissionLauncher.launch(viewModel.runtimePermissions())
            }

            state.config.displayMode == DisplayMode.OVERLAY &&
                BuildConfig.OVERLAY_SUPPORTED &&
                !state.permissions.overlayGranted -> {
                overlayLauncher.launch(
                    Intent(
                        Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                        Uri.parse("package:$packageName"),
                    ),
                )
            }

            state.config.sourceMode == SourceMode.DEVICE &&
                !state.permissions.mediaProjectionGranted -> {
                val projectionManager = getSystemService(MediaProjectionManager::class.java)
                if (projectionManager == null) {
                    pendingStart = false
                    viewModel.fail("MediaProjection is unavailable on this device.")
                    return
                }
                projectionLauncher.launch(projectionManager.createScreenCaptureIntent())
            }

            else -> {
                pendingStart = false
                viewModel.startSession(this)
            }
        }
    }

    private fun handleIntent(intent: Intent?) {
        if (intent?.getBooleanExtra(EXTRA_AUTO_START, false) == true) {
            autoStartOnResume = true
            intent.removeExtra(EXTRA_AUTO_START)
        }
    }

    companion object {
        const val EXTRA_AUTO_START = "dev.screengoated.toolbox.mobile.extra.AUTO_START"
    }
}

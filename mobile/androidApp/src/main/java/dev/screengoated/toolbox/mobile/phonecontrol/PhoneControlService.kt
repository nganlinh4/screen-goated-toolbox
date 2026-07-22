package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import androidx.core.app.NotificationCompat
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.phonecontrol.capability.PhoneControlProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayController
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayExclusion
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntime
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeCode
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeObserver
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimePhase
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.session.PhoneControlContractAssets
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolDispatcher
import dev.screengoated.toolbox.mobile.service.tryStartForegroundService

internal data class PhoneControlServiceState(
    val running: Boolean,
    val phase: PhoneControlRuntimePhase,
    val code: PhoneControlRuntimeCode,
    val userMessage: String,
    val inputCaption: String = "",
    val outputCaption: String = "",
    val listeningLevel: Float = 0f,
    val orbStateLabel: String = GeneratedPhoneControlContract.ORB_STATE_IDLE,
    val orbIconOverride: String? = null,
)

internal fun interface PhoneControlOverlayStateSink {
    fun onState(state: PhoneControlServiceState)
}

class PhoneControlService : Service() {
    private val mainHandler = Handler(Looper.getMainLooper())
    private lateinit var overlayController: PhoneControlOverlayController
    private var runtime: PhoneControlRuntime? = null
    private var preserveFailureOnDestroy = false
    private var stopReason = "system_destroy"
    private var loggedRuntimeState: Triple<Boolean, PhoneControlRuntimePhase, PhoneControlRuntimeCode>? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        Log.i(TAG, "service_created")
        overlayController = PhoneControlOverlayController(this) {
            stopRequested(source = "orb_dismiss")
        }
        PhoneControlOverlayExclusion.register(overlayController)
        ensureChannel()
        publish(
            PhoneControlServiceState(
                running = true,
                phase = PhoneControlRuntimePhase.STARTING,
                code = PhoneControlRuntimeCode.STARTING,
                userMessage = getString(R.string.phone_control_status_starting),
            ),
        )
        enterForeground()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            val source = intent.getStringExtra(EXTRA_STOP_SOURCE).orEmpty().ifBlank { "unknown" }
            Log.i(TAG, "service_command action=stop source=$source start_id=$startId")
            stopRequested(source)
        } else if (runtime == null) {
            stopReason = "runtime_terminal"
            Log.i(TAG, "service_command action=start start_id=$startId")
            startRuntime()
        } else {
            Log.i(TAG, "service_command action=duplicate_start start_id=$startId")
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        Log.i(TAG, "service_destroyed reason=$stopReason")
        runtime?.stop()
        runtime = null
        if (!preserveFailureOnDestroy) publish(stoppedState())
        PhoneControlOverlayExclusion.unregister(overlayController)
        overlayController.destroy()
        super.onDestroy()
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        overlayController.onConfigurationChanged()
    }

    private fun startRuntime() {
        try {
            val container = (application as SgtMobileApplication).appContainer
            val assets = PhoneControlContractAssets.load(this, container.json)
            val providerEvidence = PhoneControlProviderRegistry.probe(this)
            val apiKey = container.repository.currentApiKey()
            if (apiKey.isBlank()) {
                stopReason = "api_key_required"
                preserveFailureOnDestroy = true
                publish(
                    PhoneControlServiceState(
                        running = false,
                        phase = PhoneControlRuntimePhase.ERROR,
                        code = PhoneControlRuntimeCode.API_KEY_REQUIRED,
                        userMessage = getString(R.string.phone_control_status_api_key_required),
                    ),
                )
                stopSelf()
                return
            }
            lateinit var candidate: PhoneControlRuntime
            candidate = PhoneControlRuntime(
                context = this,
                httpClient = container.httpClient,
                projectionConsentStore = container.projectionConsentStore,
                apiKey = apiKey,
                voiceName = container.repository.currentGlobalTtsSettings().voice,
                contractAssets = assets,
                capabilityContext = providerEvidence.modelContext(),
                memoryRepository = container.phoneControlMemoryRepository,
                dispatchBoundary = PhoneControlToolDispatcher(this),
                observer = PhoneControlRuntimeObserver { snapshot ->
                    mainHandler.post {
                        if (runtime === candidate) publishRuntimeSnapshot(snapshot)
                    }
                },
            )
            runtime = candidate
            if (!candidate.start()) {
                stopReason = "runtime_start_rejected"
                preserveFailureOnDestroy = true
                runtime = null
                stopSelf()
            }
        } catch (error: Throwable) {
            Log.e(TAG, "service_start_failed code=configuration_failed", error)
            stopReason = "configuration_failed"
            preserveFailureOnDestroy = true
            publish(
                PhoneControlServiceState(
                    running = false,
                    phase = PhoneControlRuntimePhase.ERROR,
                    code = PhoneControlRuntimeCode.CONFIGURATION_FAILED,
                    userMessage = getString(R.string.phone_control_status_configuration_failed),
                ),
            )
            stopSelf()
        }
    }

    private fun stopRequested(source: String) {
        stopReason = "requested:$source"
        preserveFailureOnDestroy = false
        runtime?.stop()
        runtime = null
        publish(stoppedState())
        stopSelf()
    }

    private fun publishRuntimeSnapshot(snapshot: PhoneControlRuntimeSnapshot) {
        val identity = Triple(snapshot.running, snapshot.phase, snapshot.code)
        if (identity != loggedRuntimeState) {
            loggedRuntimeState = identity
            Log.i(
                TAG,
                "runtime_state running=${snapshot.running} phase=${snapshot.phase.name.lowercase()} " +
                    "code=${snapshot.code.name.lowercase()}",
            )
        }
        val state = PhoneControlServiceState(
            running = snapshot.running,
            phase = snapshot.phase,
            code = snapshot.code,
            userMessage = localizedRuntimeMessage(snapshot.code),
            inputCaption = snapshot.inputCaption,
            outputCaption = snapshot.outputCaption,
            listeningLevel = snapshot.listeningLevel,
            orbStateLabel = snapshot.orbStateLabel,
            orbIconOverride = snapshot.orbIconOverride,
        )
        publish(state)
        if (!snapshot.running && snapshot.phase == PhoneControlRuntimePhase.ERROR) {
            stopReason = "runtime_error:${snapshot.code.name.lowercase()}"
            preserveFailureOnDestroy = true
            runtime = null
            stopSelf()
        }
    }

    private fun publish(next: PhoneControlServiceState) {
        val messageChanged = mutableState.value.userMessage != next.userMessage
        mutableState.value = next
        runCatching { overlayController.onState(next) }
            .onFailure { Log.e(TAG, "overlay_state_sink_failed", it) }
        if (messageChanged) {
            getSystemService(NotificationManager::class.java).notify(
                NOTIFICATION_ID,
                buildNotification(next.userMessage),
            )
        }
    }

    private fun enterForeground() {
        val notification = buildNotification(getString(R.string.phone_control_status_starting))
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            val serviceTypes = ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE or
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE or
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
            startForeground(NOTIFICATION_ID, notification, serviceTypes)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun buildNotification(message: String): Notification {
        val openIntent = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val stopIntent = PendingIntent.getService(
            this,
            1,
            Intent(this, PhoneControlService::class.java)
                .setAction(ACTION_STOP)
                .putExtra(EXTRA_STOP_SOURCE, "notification"),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_qs_tile)
            .setContentTitle(getString(R.string.phone_control_title))
            .setContentText(message)
            .setContentIntent(openIntent)
            .addAction(0, getString(R.string.notification_action_stop), stopIntent)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .build()
    }

    private fun ensureChannel() {
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                getString(R.string.phone_control_channel_name),
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = getString(R.string.phone_control_channel_description)
            },
        )
    }

    private fun localizedRuntimeMessage(code: PhoneControlRuntimeCode): String = getString(
        when (code) {
            PhoneControlRuntimeCode.STOPPED -> R.string.phone_control_status_stopped
            PhoneControlRuntimeCode.STARTING -> R.string.phone_control_status_starting
            PhoneControlRuntimeCode.CONNECTING -> R.string.phone_control_status_connecting
            PhoneControlRuntimeCode.READY -> R.string.phone_control_status_ready
            PhoneControlRuntimeCode.WORKING -> R.string.phone_control_status_working
            PhoneControlRuntimeCode.FINALIZING -> R.string.phone_control_status_finalizing
            PhoneControlRuntimeCode.RECONNECTING -> R.string.phone_control_status_reconnecting
            PhoneControlRuntimeCode.ACCESSIBILITY_UNAVAILABLE ->
                R.string.phone_control_status_accessibility_unavailable
            PhoneControlRuntimeCode.SCREEN_CAPTURE_FAILED ->
                R.string.phone_control_status_capture_failed
            PhoneControlRuntimeCode.TOOL_RECONCILIATION_REQUIRED ->
                R.string.phone_control_status_reconciliation_required
            PhoneControlRuntimeCode.API_KEY_REQUIRED ->
                R.string.phone_control_status_api_key_required
            PhoneControlRuntimeCode.CONFIGURATION_FAILED ->
                R.string.phone_control_status_configuration_failed
            PhoneControlRuntimeCode.MICROPHONE_FAILED ->
                R.string.phone_control_status_microphone_failed
            PhoneControlRuntimeCode.TRANSPORT_FAILED ->
                R.string.phone_control_status_transport_failed
            PhoneControlRuntimeCode.RUNTIME_FAILED -> R.string.phone_control_status_runtime_failed
        },
    )

    private fun stoppedState() = PhoneControlServiceState(
        running = false,
        phase = PhoneControlRuntimePhase.STOPPED,
        code = PhoneControlRuntimeCode.STOPPED,
        userMessage = getString(R.string.phone_control_status_stopped),
    )

    companion object {
        private const val TAG = "SGTPhoneControlService"
        private const val CHANNEL_ID = "phone_control"
        private const val NOTIFICATION_ID = 4081
        private const val ACTION_START = "dev.screengoated.toolbox.mobile.phonecontrol.START"
        private const val ACTION_STOP = "dev.screengoated.toolbox.mobile.phonecontrol.STOP"
        private const val EXTRA_STOP_SOURCE = "dev.screengoated.toolbox.mobile.phonecontrol.STOP_SOURCE"

        private val mutableState = mutableStateOf(
            PhoneControlServiceState(
                running = false,
                phase = PhoneControlRuntimePhase.STOPPED,
                code = PhoneControlRuntimeCode.STOPPED,
                userMessage = "",
            ),
        )
        internal val state: State<PhoneControlServiceState> = mutableState

        fun start(context: Context): Boolean = tryStartForegroundService(
            context,
            Intent(context, PhoneControlService::class.java).setAction(ACTION_START),
            "PhoneControlService",
        )

        fun stop(context: Context) {
            dispatchStop(context, source = "app")
        }

        private fun dispatchStop(context: Context, source: String) {
            context.startService(
                Intent(context, PhoneControlService::class.java)
                    .setAction(ACTION_STOP)
                    .putExtra(EXTRA_STOP_SOURCE, source),
            )
        }
    }
}

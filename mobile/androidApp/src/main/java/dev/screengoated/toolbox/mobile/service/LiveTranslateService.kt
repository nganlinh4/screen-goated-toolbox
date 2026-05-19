package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationManagerCompat
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import java.util.concurrent.atomic.AtomicBoolean

class LiveTranslateService : androidx.lifecycle.LifecycleService() {
    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val stopping = AtomicBoolean(false)

    private lateinit var runtime: LiveSessionRuntime
    private lateinit var notifications: ServiceNotificationFactory
    private lateinit var repository: dev.screengoated.toolbox.mobile.model.AndroidLiveSessionRepository
    private lateinit var projectionConsentStore: dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore

    override fun onCreate() {
        super.onCreate()

        val container = (application as SgtMobileApplication).appContainer
        repository = container.repository
        projectionConsentStore = container.projectionConsentStore
        notifications = ServiceNotificationFactory(this)
        notifications.ensureChannel()
        runtime = LiveSessionRuntime(
            context = this,
            repository = repository,
            projectionConsentStore = container.projectionConsentStore,
            liveSocketClient = container.geminiLiveSocketClient,
            s2sClient = container.geminiS2sClient,
            translationClient = container.realtimeTranslationClient,
            ttsRuntimeService = container.ttsRuntimeService,
            overlaySupported = dev.screengoated.toolbox.mobile.BuildConfig.OVERLAY_SUPPORTED,
            stopRequested = { stopSession() },
            sourceModeChanged = ::updateForegroundType,
        )

        serviceScope.launch {
            repository.state
                .map(notifications::snapshot)
                .distinctUntilChanged()
                .collectLatest { snapshot ->
                    try {
                        NotificationManagerCompat.from(this@LiveTranslateService).notify(
                            ServiceNotificationFactory.NOTIFICATION_ID,
                            notifications.build(snapshot),
                        )
                    } catch (e: SecurityException) {
                        Log.w(TAG, "Skipping live translate notification update without permission", e)
                    }
                }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        when (intent?.action) {
            ACTION_STOP -> stopSession()
            else -> startSession()
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        runtime.stop()
        repository.clearTransientSessionConfig()
        (application as SgtMobileApplication).appContainer.audioPresetLaunchStore.setActiveRealtimePresetId(null)
        serviceScope.cancel()
        super.onDestroy()
    }

    private fun startSession() {
        if (stopping.get()) {
            return
        }
        repository.ensureSafePlayDefaults()
        repository.refreshPermissions()
        if (!repository.canStartSession()) {
            repository.markAwaitingPermissions()
            startMicrophoneForeground(notifications.build(notifications.snapshot(repository.state.value)))
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return
        }

        val notification = notifications.build(notifications.snapshot(repository.state.value))
        try {
            startSessionForeground(notification, repository.state.value.config.sourceMode)
        } catch (_: SecurityException) {
            projectionConsentStore.clear()
            startMicrophoneForeground(notification)
        }
        runtime.start(serviceScope)
    }

    private fun stopSession() {
        if (!stopping.compareAndSet(false, true)) {
            return
        }
        runtime.stop()
        repository.commitPendingLiveHistory()
        repository.stop()
        repository.clearTransientSessionConfig()
        (application as SgtMobileApplication).appContainer.audioPresetLaunchStore.setActiveRealtimePresetId(null)
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun updateForegroundType(sourceMode: SourceMode) {
        val notification = notifications.build(notifications.snapshot(repository.state.value))
        try {
            startSessionForeground(notification, sourceMode)
        } catch (_: SecurityException) {
            projectionConsentStore.clear()
            startMicrophoneForeground(notification)
        }
    }

    private fun startSessionForeground(notification: android.app.Notification, sourceMode: SourceMode) {
        if (sourceMode == SourceMode.DEVICE && projectionConsentStore.hasConsent()) {
            startForeground(
                ServiceNotificationFactory.NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION,
            )
        } else {
            startMicrophoneForeground(notification)
        }
    }

    private fun startMicrophoneForeground(notification: android.app.Notification) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            startForeground(
                ServiceNotificationFactory.NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
            )
        } else {
            startForeground(ServiceNotificationFactory.NOTIFICATION_ID, notification)
        }
    }

    companion object {
        private const val TAG = "LiveTranslateService"

        const val ACTION_STOP = "dev.screengoated.toolbox.mobile.action.STOP"

        fun start(context: Context): Boolean {
            val intent = Intent(context, LiveTranslateService::class.java)
            return tryStartForegroundService(context, intent, TAG)
        }

        fun stop(context: Context) {
            val intent = Intent(context, LiveTranslateService::class.java)
                .setAction(ACTION_STOP)
            context.startService(intent)
        }
    }
}

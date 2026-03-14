package dev.screengoated.toolbox.mobile.service

import android.content.pm.ServiceInfo
import android.content.Context
import android.content.Intent
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

class LiveTranslateService : androidx.lifecycle.LifecycleService() {
    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

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
            translationClient = container.realtimeTranslationClient,
            ttsRuntimeService = container.ttsRuntimeService,
            overlaySupported = dev.screengoated.toolbox.mobile.BuildConfig.OVERLAY_SUPPORTED,
            stopRequested = { stopSession() },
            sourceModeChanged = ::updateForegroundType,
            parakeetModelManager = container.parakeetModelManager,
        )

        serviceScope.launch {
            repository.state
                .map(notifications::snapshot)
                .distinctUntilChanged()
                .collectLatest { snapshot ->
                NotificationManagerCompat.from(this@LiveTranslateService).notify(
                    ServiceNotificationFactory.NOTIFICATION_ID,
                    notifications.build(snapshot),
                )
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> stopSession()
            else -> startSession()
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        runtime.stop()
        serviceScope.cancel()
        super.onDestroy()
    }

    private fun startSession() {
        repository.ensureSafePlayDefaults()
        repository.refreshPermissions()
        if (!repository.canStartSession()) {
            repository.markAwaitingPermissions()
            startForeground(
                ServiceNotificationFactory.NOTIFICATION_ID,
                notifications.build(notifications.snapshot(repository.state.value)),
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
            )
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return
        }

        val notification = notifications.build(notifications.snapshot(repository.state.value))
        val fgsType = resolveForegroundType(repository.state.value.config.sourceMode)
        try {
            startForeground(ServiceNotificationFactory.NOTIFICATION_ID, notification, fgsType)
        } catch (_: SecurityException) {
            projectionConsentStore.clear()
            startForeground(
                ServiceNotificationFactory.NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
            )
        }
        runtime.start(serviceScope)
    }

    private fun stopSession() {
        runtime.stop()
        repository.stop()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun updateForegroundType(sourceMode: SourceMode) {
        val notification = notifications.build(notifications.snapshot(repository.state.value))
        val fgsType = resolveForegroundType(sourceMode)
        try {
            startForeground(ServiceNotificationFactory.NOTIFICATION_ID, notification, fgsType)
        } catch (_: SecurityException) {
            projectionConsentStore.clear()
            startForeground(
                ServiceNotificationFactory.NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
            )
        }
    }

    private fun resolveForegroundType(sourceMode: SourceMode): Int {
        return if (sourceMode == SourceMode.DEVICE && projectionConsentStore.hasConsent()) {
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION
        } else {
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
        }
    }

    companion object {
        const val ACTION_STOP = "dev.screengoated.toolbox.mobile.action.STOP"

        fun start(context: Context) {
            val intent = Intent(context, LiveTranslateService::class.java)
            context.startForegroundService(intent)
        }

        fun stop(context: Context) {
            val intent = Intent(context, LiveTranslateService::class.java)
                .setAction(ACTION_STOP)
            context.startService(intent)
        }
    }
}

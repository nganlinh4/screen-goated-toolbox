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

        val container = (application as SgtMobileApplication).appContainerOrNull
        if (container == null) {
            stopForUnavailableSecureStore()
            return
        }
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
            toastBus = container.toastBus,
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
        if (!::runtime.isInitialized) {
            stopForUnavailableSecureStore(startId)
            return START_NOT_STICKY
        }
        when (intent?.action) {
            ACTION_STOP -> stopSession()
            else -> startSession()
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        if (::runtime.isInitialized) {
            runtime.stop()
            runtime.dispose()
        }
        serviceScope.cancel()
        super.onDestroy()
    }

    private fun startSession() {
        if (stopping.get()) {
            return
        }
        repository.ensureSafePlayDefaults()
        repository.refreshPermissions()
        val notification = notifications.build(notifications.snapshot(repository.state.value))
        if (!repository.canStartSession()) {
            // Missing permissions: settle the accepted start without a microphone type,
            // which API 34+ rejects without RECORD_AUDIO.
            repository.markAwaitingPermissions()
            settleWithoutSession(notification)
            return
        }

        try {
            startSessionForeground(notification, repository.state.value.config.sourceMode)
        } catch (_: SecurityException) {
            projectionConsentStore.clear()
            try {
                startMicrophoneForeground(notification)
            } catch (error: RuntimeException) {
                Log.w(TAG, "Microphone foreground fallback was refused", error)
                settleWithoutSession(notification)
                return
            }
        }
        runtime.start(serviceScope)
    }

    private fun settleWithoutSession(notification: android.app.Notification) {
        settleUnavailableForeground(
            promote = { startPermissionFreeForeground(ServiceNotificationFactory.NOTIFICATION_ID, notification) },
            remove = { stopForeground(STOP_FOREGROUND_REMOVE) },
            stop = { stopSelf() },
            failure = { error -> Log.w(TAG, "Unable to settle the foreground start", error) },
        )
    }

    private fun stopSession() {
        if (!stopping.compareAndSet(false, true)) {
            return
        }
        runtime.stop()
        repository.commitPendingLiveHistory()
        repository.stop()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun updateForegroundType(sourceMode: SourceMode) {
        val notification = notifications.build(notifications.snapshot(repository.state.value))
        try {
            startSessionForeground(notification, sourceMode)
        } catch (_: SecurityException) {
            projectionConsentStore.clear()
            try {
                startMicrophoneForeground(notification)
            } catch (error: RuntimeException) {
                Log.w(TAG, "Microphone foreground fallback was refused", error)
                stopSession()
            }
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

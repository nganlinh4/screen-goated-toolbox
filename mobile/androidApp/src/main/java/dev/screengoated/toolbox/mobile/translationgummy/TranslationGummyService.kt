package dev.screengoated.toolbox.mobile.translationgummy

import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationManagerCompat
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.service.tryStartForegroundService
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch

class TranslationGummyService : androidx.lifecycle.LifecycleService() {
    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    private lateinit var appContainer: dev.screengoated.toolbox.mobile.AppContainer
    private lateinit var repository: TranslationGummyRepository
    private lateinit var runtime: TranslationGummyRuntime
    private lateinit var notifications: TranslationGummyNotificationFactory

    override fun onCreate() {
        super.onCreate()
        appContainer = (application as SgtMobileApplication).appContainer
        repository = appContainer.translationGummyRepository
        runtime = appContainer.translationGummyRuntime
        notifications = TranslationGummyNotificationFactory(this, repository::localeText)
        notifications.ensureChannel()

        serviceScope.launch {
            repository.state.collectLatest { state ->
                try {
                    NotificationManagerCompat.from(this@TranslationGummyService).notify(
                        TranslationGummyNotificationFactory.NOTIFICATION_ID,
                        notifications.build(state),
                    )
                } catch (e: SecurityException) {
                    Log.w(TAG, "Skipping translation gummy notification update without permission", e)
                }
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        when (intent?.action) {
            ACTION_STOP -> stopSession()
            ACTION_RESTART -> restartSession()
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
        // Must call startForeground before anything else after startForegroundService()
        startMicrophoneForeground()
        if (!repository.currentAppliedConfig().isValid()) {
            repository.markNotConfigured()
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return
        }
        if (repository.currentApiKey().isBlank()) {
            val message = repository.localeText().translationGummyApiKeyRequired
            repository.fail(message)
            appContainer.toastBus.show(message)
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return
        }
        runtime.start(serviceScope)
    }

    private fun restartSession() {
        startMicrophoneForeground()
        if (!repository.currentAppliedConfig().isValid()) {
            repository.markNotConfigured()
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return
        }
        if (repository.currentApiKey().isBlank()) {
            val message = repository.localeText().translationGummyApiKeyRequired
            repository.fail(message)
            appContainer.toastBus.show(message)
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return
        }
        runtime.restart(serviceScope)
    }

    private fun startMicrophoneForeground() {
        val notification = notifications.build(repository.state.value)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            startForeground(
                TranslationGummyNotificationFactory.NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
            )
        } else {
            startForeground(TranslationGummyNotificationFactory.NOTIFICATION_ID, notification)
        }
    }

    private fun stopSession() {
        runtime.stop()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    companion object {
        private const val TAG = "TranslationGummyService"

        const val ACTION_STOP = "dev.screengoated.toolbox.mobile.action.TRANSLATION_GUMMY_STOP"
        const val ACTION_RESTART = "dev.screengoated.toolbox.mobile.action.TRANSLATION_GUMMY_RESTART"

        fun start(context: Context, restart: Boolean = false): Boolean {
            val intent = Intent(context, TranslationGummyService::class.java)
            if (restart) {
                intent.action = ACTION_RESTART
            }
            return tryStartForegroundService(context, intent, TAG)
        }

        fun stop(context: Context) {
            context.startService(
                Intent(context, TranslationGummyService::class.java)
                    .setAction(ACTION_STOP),
            )
        }
    }
}

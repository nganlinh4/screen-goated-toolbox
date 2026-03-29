package dev.screengoated.toolbox.mobile.bilingualrelay

import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import androidx.core.app.NotificationManagerCompat
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch

class BilingualRelayService : androidx.lifecycle.LifecycleService() {
    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    private lateinit var repository: BilingualRelayRepository
    private lateinit var runtime: BilingualRelayRuntime
    private lateinit var notifications: BilingualRelayNotificationFactory

    override fun onCreate() {
        super.onCreate()
        val container = (application as SgtMobileApplication).appContainer
        repository = container.bilingualRelayRepository
        runtime = container.bilingualRelayRuntime
        notifications = BilingualRelayNotificationFactory(this, repository::localeText)
        notifications.ensureChannel()

        serviceScope.launch {
            repository.state.collectLatest { state ->
                NotificationManagerCompat.from(this@BilingualRelayService).notify(
                    BilingualRelayNotificationFactory.NOTIFICATION_ID,
                    notifications.build(state),
                )
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
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
        if (!repository.currentAppliedConfig().isValid()) {
            repository.markNotConfigured()
            stopSelf()
            return
        }
        if (repository.currentApiKey().isBlank()) {
            repository.fail(repository.localeText().bilingualRelayApiKeyRequired)
            stopSelf()
            return
        }
        startForeground(
            BilingualRelayNotificationFactory.NOTIFICATION_ID,
            notifications.build(repository.state.value),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
        )
        runtime.start(serviceScope)
    }

    private fun restartSession() {
        if (!repository.currentAppliedConfig().isValid()) {
            repository.markNotConfigured()
            stopSelf()
            return
        }
        if (repository.currentApiKey().isBlank()) {
            repository.fail(repository.localeText().bilingualRelayApiKeyRequired)
            stopSelf()
            return
        }
        startForeground(
            BilingualRelayNotificationFactory.NOTIFICATION_ID,
            notifications.build(repository.state.value),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
        )
        runtime.restart(serviceScope)
    }

    private fun stopSession() {
        runtime.stop()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    companion object {
        const val ACTION_STOP = "dev.screengoated.toolbox.mobile.action.BILINGUAL_RELAY_STOP"
        const val ACTION_RESTART = "dev.screengoated.toolbox.mobile.action.BILINGUAL_RELAY_RESTART"

        fun start(context: Context, restart: Boolean = false) {
            val intent = Intent(context, BilingualRelayService::class.java)
            if (restart) {
                intent.action = ACTION_RESTART
            }
            context.startForegroundService(intent)
        }

        fun stop(context: Context) {
            context.startService(
                Intent(context, BilingualRelayService::class.java)
                    .setAction(ACTION_STOP),
            )
        }
    }
}

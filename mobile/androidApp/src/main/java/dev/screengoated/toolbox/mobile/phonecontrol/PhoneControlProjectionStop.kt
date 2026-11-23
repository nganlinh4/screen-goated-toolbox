package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.service.tryStartForegroundService

internal const val ACTION_PHONE_CONTROL_STOP = "dev.screengoated.toolbox.mobile.phonecontrol.STOP"
internal const val EXTRA_STOP_SOURCE = "dev.screengoated.toolbox.mobile.phonecontrol.STOP_SOURCE"
internal const val EXTRA_STOP_BOUNDARY = "dev.screengoated.toolbox.mobile.phonecontrol.STOP_BOUNDARY"
internal const val EXTRA_STOP_SEQUENCE = "dev.screengoated.toolbox.mobile.phonecontrol.STOP_SEQUENCE"

internal data class PhoneControlStopStamp(val boundary: String, val sequence: Long)

internal class DeferredPhoneControlStop {
    private var pending: PhoneControlStopStamp? = null

    fun defer(intent: Intent?) {
        if (intent?.action != ACTION_PHONE_CONTROL_STOP || !intent.hasExtra(EXTRA_STOP_SEQUENCE)) return
        val boundary = intent.getStringExtra(EXTRA_STOP_BOUNDARY)?.takeIf(String::isNotBlank) ?: return
        val stamp = PhoneControlStopStamp(boundary, intent.getLongExtra(EXTRA_STOP_SEQUENCE, 0))
        PhoneControlProjectionDispatches.publishForCurrentBoundary(stamp.boundary, stamp.sequence) {
            pending = stamp
        }
    }

    fun replay(send: (PhoneControlStopStamp) -> Unit) {
        val stamp = pending ?: return
        pending = null
        PhoneControlProjectionDispatches.publishForCurrentBoundary(stamp.boundary, stamp.sequence) {
            send(stamp)
        }
    }

    fun replay(context: Context) = replay { stamp ->
        val intent = phoneControlStopIntent(context, "notification", stamp.boundary)
            .putExtra(EXTRA_STOP_SEQUENCE, stamp.sequence)
        if (!tryStartForegroundService(context, intent, "PhoneControlService")) {
            runCatching { PhoneControlSetupNotification.showStopRetry(context, stamp.boundary) }
            context.showPhoneControlToast(R.string.phone_control_stop_retry)
        }
    }
}

internal fun stampNotificationStop(boundary: String?, currentSequence: () -> Long): PhoneControlStopStamp? =
    boundary?.takeIf(String::isNotBlank)?.let { PhoneControlStopStamp(it, currentSequence()) }

internal fun promoteStopOnlyService(
    service: PhoneControlService,
    notification: PhoneControlSessionNotification,
    intent: Intent,
    startId: Int,
): Boolean {
    if (runCatching {
        notification.enterStopForeground(service.phoneControlString(R.string.phone_control_status_starting))
    }.isSuccess) return true
    val boundary = intent.getStringExtra(EXTRA_STOP_BOUNDARY)
    val retryBoundary = service.requestServiceTeardown(startId, boundary)
    if (retryBoundary != null) {
        runCatching {
            PhoneControlSetupNotification.showStopRetry(service, retryBoundary)
        }
        service.showPhoneControlToast(R.string.phone_control_stop_retry)
    }
    return false
}

/** Receives only this app's immutable notification action and stamps it at delivery. */
class PhoneControlStopReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION_PHONE_CONTROL_STOP) return
        val app = context.applicationContext as? dev.screengoated.toolbox.mobile.SgtMobileApplication
        if (app?.appContainerOrNull == null) return
        val stamp = stampNotificationStop(intent.getStringExtra(EXTRA_STOP_BOUNDARY)) {
            PhoneControlProjectionDispatches.currentIntentSequence()
        } ?: return
        val serviceIntent = phoneControlStopIntent(context, "notification", stamp.boundary)
            .putExtra(EXTRA_STOP_SEQUENCE, stamp.sequence)
        if (!tryStartForegroundService(context, serviceIntent, "PhoneControlService")) {
            runCatching { PhoneControlSetupNotification.showStopRetry(context, stamp.boundary) }
            context.showPhoneControlToast(R.string.phone_control_stop_retry)
        }
    }
}

internal fun phoneControlStopIntent(
    context: Context,
    source: String,
    boundary: String = PhoneControlProjectionDispatches.stopBoundary(),
): Intent =
    Intent(context, PhoneControlService::class.java)
        .setAction(ACTION_PHONE_CONTROL_STOP)
        .putExtra(EXTRA_STOP_SOURCE, source)
        .putExtra(EXTRA_STOP_BOUNDARY, boundary)
        .putExtra(EXTRA_STOP_SEQUENCE, PhoneControlProjectionDispatches.currentIntentSequence())

internal fun phoneControlStopPendingIntent(
    context: Context,
    boundary: String = PhoneControlProjectionDispatches.stopBoundary(),
    requestCode: Int = 1,
): PendingIntent =
    PendingIntent.getBroadcast(
        context,
        requestCode,
        Intent(context, PhoneControlStopReceiver::class.java)
            .setAction(ACTION_PHONE_CONTROL_STOP)
            .putExtra(EXTRA_STOP_BOUNDARY, boundary),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )

internal fun reservePhoneControlProjectionConsent(context: Context): String? {
    val app = context.applicationContext
    return runCatching {
        PhoneControlProjectionDispatches.reserveConsent { boundary ->
            phoneControlStopPendingIntent(app, boundary)
            // A failed stop belongs to the previous generation. Publication
            // and failure handling share the arbiter lock with this update.
            runCatching { PhoneControlSetupNotification.clearStopRetry(app) }
        }
    }.getOrNull()
}

internal fun newPhoneControlProjectionDispatch(
    context: Context,
    consentGeneration: String,
): PhoneControlProjectionDispatch? {
    val app = context.applicationContext
    return runCatching {
        PhoneControlProjectionDispatches.create(
            consentGeneration = consentGeneration,
            publishOrphanedReentry = { intent ->
                PhoneControlSetupNotification.showOrphanedReentry(
                    app,
                    app.phoneControlString(R.string.phone_control_private_setup_pending),
                    intent,
                )
            },
        )
    }.getOrNull()
}

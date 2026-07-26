package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.app.ActivityOptions
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.annotation.RequiresApi
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import java.util.concurrent.atomic.AtomicLong

internal enum class PhoneControlBackgroundLaunchMode {
    PLATFORM_DEFAULT,
    ALLOWED,
    ALLOW_ALWAYS,
}

internal fun phoneControlBackgroundLaunchMode(apiLevel: Int): PhoneControlBackgroundLaunchMode =
    when {
        apiLevel >= 36 -> PhoneControlBackgroundLaunchMode.ALLOW_ALWAYS
        apiLevel >= 34 -> PhoneControlBackgroundLaunchMode.ALLOWED
        else -> PhoneControlBackgroundLaunchMode.PLATFORM_DEFAULT
    }

internal data class PhoneControlCoordinatorReentryDispatch(
    val token: Long,
    val dispatched: Boolean,
)

internal enum class PhoneControlExternalResultDisposition {
    HANDLE,
    IGNORE_RETIRED,
    RETIRE_FOR_REENTRY,
}

internal fun phoneControlExternalResultDisposition(
    reentryPending: Boolean,
    externalStepActive: Boolean,
): PhoneControlExternalResultDisposition = when {
    reentryPending -> PhoneControlExternalResultDisposition.RETIRE_FOR_REENTRY
    !externalStepActive -> PhoneControlExternalResultDisposition.IGNORE_RETIRED
    else -> PhoneControlExternalResultDisposition.HANDLE
}

internal object PhoneControlCoordinatorReentryLauncher {
    private val nextToken = AtomicLong(0L)
    private val expectedToken = AtomicLong(NO_TOKEN)

    fun dispatch(context: Context, intent: Intent): PhoneControlCoordinatorReentryDispatch {
        require(intent.component?.packageName == context.packageName) {
            "coordinator reentry must target this application explicitly"
        }
        val token = nextToken.updateAndGet { current ->
            if (current == Long.MAX_VALUE) 1L else current + 1L
        }
        expectedToken.set(token)
        val target = Intent(intent).putExtra(EXTRA_REENTRY_TOKEN, token)
        val mode = phoneControlBackgroundLaunchMode(Build.VERSION.SDK_INT)
        val dispatched = runCatching {
            val pendingIntent = PendingIntent.getActivity(
                context,
                REENTRY_REQUEST_CODE,
                target,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
                creatorOptions(mode),
            )
            pendingIntent.send(
                context,
                0,
                null,
                null,
                null,
                null,
                senderOptions(mode),
            )
        }.isSuccess
        if (!dispatched) expectedToken.compareAndSet(token, NO_TOKEN)
        return PhoneControlCoordinatorReentryDispatch(token, dispatched)
    }

    fun acknowledge(intent: Intent, mode: String) {
        val token = intent.getLongExtra(EXTRA_REENTRY_TOKEN, NO_TOKEN)
        val accepted = token != NO_TOKEN && expectedToken.compareAndSet(token, NO_TOKEN)
        if (token != NO_TOKEN) {
            PhoneControlLog.i(
                TAG,
                "coordinator_reentry_ack reentry_sequence=$token " +
                    "accepted=$accepted mode=$mode",
            )
        }
    }

    fun hasPendingReceipt(): Boolean = expectedToken.get() != NO_TOKEN

    private fun creatorOptions(mode: PhoneControlBackgroundLaunchMode): Bundle? =
        if (mode != PhoneControlBackgroundLaunchMode.PLATFORM_DEFAULT &&
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE
        ) {
            creatorOptionsApi34(mode)
        } else {
            null
        }

    private fun senderOptions(mode: PhoneControlBackgroundLaunchMode): Bundle? =
        if (mode != PhoneControlBackgroundLaunchMode.PLATFORM_DEFAULT &&
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE
        ) {
            senderOptionsApi34(mode)
        } else {
            null
        }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    private fun creatorOptionsApi34(mode: PhoneControlBackgroundLaunchMode): Bundle =
        ActivityOptions.makeBasic().apply {
            setPendingIntentCreatorBackgroundActivityStartMode(backgroundLaunchModeApi34(mode))
        }.toBundle()

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    private fun senderOptionsApi34(mode: PhoneControlBackgroundLaunchMode): Bundle =
        ActivityOptions.makeBasic().apply {
            setPendingIntentBackgroundActivityStartMode(backgroundLaunchModeApi34(mode))
        }.toBundle()

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    @Suppress("DEPRECATION")
    private fun backgroundLaunchModeApi34(mode: PhoneControlBackgroundLaunchMode): Int =
        if (mode == PhoneControlBackgroundLaunchMode.ALLOW_ALWAYS &&
            Build.VERSION.SDK_INT >= 36
        ) {
            backgroundLaunchModeApi36()
        } else {
            ActivityOptions.MODE_BACKGROUND_ACTIVITY_START_ALLOWED
        }

    @RequiresApi(36)
    private fun backgroundLaunchModeApi36(): Int =
        ActivityOptions.MODE_BACKGROUND_ACTIVITY_START_ALLOW_ALWAYS

    private const val TAG = "SGTPhoneControlActivation"
    private const val REENTRY_REQUEST_CODE = 0x5347
    private const val NO_TOKEN = -1L
    private const val EXTRA_REENTRY_TOKEN =
        "dev.screengoated.toolbox.mobile.phonecontrol.COORDINATOR_REENTRY_TOKEN"
}

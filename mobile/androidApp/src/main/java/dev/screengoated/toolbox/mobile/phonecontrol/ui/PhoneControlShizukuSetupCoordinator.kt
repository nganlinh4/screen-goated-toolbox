package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlService
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlSetupNotification
import dev.screengoated.toolbox.mobile.phonecontrol.phoneControlString
import dev.screengoated.toolbox.mobile.phonecontrol.showPhoneControlToast
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PlatformUserStepSlot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuBridgeCondition
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuCommandBridge
import java.io.Closeable
import rikka.shizuku.Shizuku

internal class PhoneControlShizukuSetupCoordinator(
    private val activity: ComponentActivity,
    private val externalStep: PlatformUserStepSlot,
    private val permissionStep: PlatformUserStepSlot,
    private val launchExternal: (Intent) -> Unit,
    private val finishActivity: () -> Unit,
) : Closeable {
    private var lastAttempt: PhoneControlShizukuSetupAttempt? = null
    private var externalStepActive = false
    private var registered = false
    private var closed = false
    private var powerObserver: Closeable? = null

    private val packageReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (intent?.data?.schemeSpecificPart != SHIZUKU_PACKAGE) return
            onProviderEvent("package_changed")
        }
    }

    private val permissionListener =
        Shizuku.OnRequestPermissionResultListener { requestCode, _ ->
            if (requestCode != SHIZUKU_PERMISSION_REQUEST) return@OnRequestPermissionResultListener
            activity.runOnUiThread {
                if (closed) return@runOnUiThread
                permissionStep.finish()
                advance("permission_result")
            }
        }

    private val binderReceivedListener = Shizuku.OnBinderReceivedListener {
        activity.runOnUiThread {
            if (closed) return@runOnUiThread
            onProviderEvent("binder_received")
        }
    }

    fun start(savedState: Bundle?) {
        check(!registered) { "Shizuku setup coordinator already started" }
        lastAttempt = savedState?.shizukuAttempt()
        externalStepActive = savedState?.getBoolean(
            STATE_EXTERNAL_ACTIVE,
            false,
        ) ?: false
        ContextCompat.registerReceiver(
            activity,
            packageReceiver,
            IntentFilter().apply {
                addAction(Intent.ACTION_PACKAGE_ADDED)
                addAction(Intent.ACTION_PACKAGE_REPLACED)
                addDataScheme("package")
            },
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        powerObserver = PhoneControlPowerPreferences.observe(activity) { choice ->
            if (choice != PhoneControlPowerChoice.SHIZUKU) {
                activity.runOnUiThread(::cancelForAuthorityChange)
            }
        }
        Shizuku.addRequestPermissionResultListener(permissionListener)
        Shizuku.addBinderReceivedListener(binderReceivedListener)
        registered = true
        advance("direct")
    }

    fun onExternalReturn() {
        if (closed) return
        retireExternalStep()
        advance("external_return")
    }

    fun save(outState: Bundle) {
        lastAttempt?.let { attempt ->
            outState.putString(STATE_CONDITION, attempt.condition.wireName)
            outState.putString(STATE_ACTION, attempt.action.wireName)
        }
        outState.putBoolean(STATE_EXTERNAL_ACTIVE, externalStepActive)
    }

    override fun close() {
        if (closed) return
        closed = true
        powerObserver?.close()
        powerObserver = null
        if (registered) {
            runCatching { activity.unregisterReceiver(packageReceiver) }
            Shizuku.removeRequestPermissionResultListener(permissionListener)
            Shizuku.removeBinderReceivedListener(binderReceivedListener)
            registered = false
        }
    }

    private fun onProviderEvent(trigger: String) {
        if (PhoneControlPowerPreferences.current(activity) != PhoneControlPowerChoice.SHIZUKU) {
            cancelForAuthorityChange()
            return
        }
        PhoneControlLog.i(
            TAG,
            "authority_setup_event provider=shizuku trigger=$trigger " +
                "external_step_active=$externalStepActive",
        )
        retireExternalStep()
        advance(trigger)
    }

    private fun advance(trigger: String) {
        if (closed || activity.isFinishing || activity.isDestroyed) return
        if (PhoneControlService.captureSuspended) {
            PhoneControlLog.i(
                TAG,
                "authority_setup_deferred provider=shizuku reason=protected_checkpoint",
            )
            finishActivity()
            return
        }
        if (PhoneControlPowerPreferences.current(activity) != PhoneControlPowerChoice.SHIZUKU) {
            cancelForAuthorityChange()
            return
        }
        val probe = ShizukuCommandBridge.probe(activity)
        val action = nextPhoneControlShizukuSetupAction(probe)
        val attempt = PhoneControlShizukuSetupAttempt(probe.condition, action)
        PhoneControlLog.i(
            TAG,
            "authority_setup_step provider=shizuku trigger=$trigger " +
                "condition=${probe.condition.wireName} action=${action.wireName}",
        )
        when (action) {
            PhoneControlShizukuSetupAction.COMPLETE -> complete()
            PhoneControlShizukuSetupAction.REQUEST_PERMISSION ->
                requestPermission(attempt, trigger)
            PhoneControlShizukuSetupAction.OPEN_MANAGER,
            PhoneControlShizukuSetupAction.OPEN_STORE,
            -> openExternalStep(probe.condition, attempt, trigger)
        }
    }

    private fun complete() {
        retireExternalStep()
        permissionStep.finish()
        val continuation = when {
            PhoneControlService.captureSuspended ->
                PhoneControlActivity.resumeCaptureIntent(activity)
            !PhoneControlService.state.value.running ->
                PhoneControlActivity.activationIntent(activity)
            else -> null
        }
        PhoneControlService.clearAuthoritySetup(activity, SHIZUKU_PROVIDER_ID)
        activity.showPhoneControlToast(R.string.phone_control_shizuku_ready_toast)
        PhoneControlLog.i(TAG, "authority_setup_result provider=shizuku ready=true")
        if (continuation != null) {
            if (activity.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) {
                activity.startActivity(continuation)
            } else {
                PhoneControlSetupNotification.show(
                    activity,
                    activity.phoneControlString(R.string.phone_control_shizuku_ready_resume),
                    continuation,
                )
            }
        }
        finishActivity()
    }

    private fun requestPermission(
        attempt: PhoneControlShizukuSetupAttempt,
        trigger: String,
    ) {
        when (
            phoneControlShizukuRepeatDisposition(
                attempt,
                lastAttempt,
                permissionStep.active,
            )
        ) {
            PhoneControlShizukuRepeatDisposition.WAIT_FOR_EVENT -> {
                PhoneControlLog.i(
                    TAG,
                    "authority_setup_waiting provider=shizuku " +
                        "condition=${attempt.condition.wireName}",
                )
                return
            }
            PhoneControlShizukuRepeatDisposition.LEAVE_SELECTED_PENDING -> {
                leavePending(attempt.condition)
                return
            }
            PhoneControlShizukuRepeatDisposition.DISPATCH -> Unit
        }
        lastAttempt = attempt
        reportGuidance(
            R.string.phone_control_shizuku_request_permission,
            requestAutomation = false,
            captureHandoffAfterAutomation = false,
        )
        activity.showPhoneControlToast(
            R.string.phone_control_shizuku_request_permission_toast,
        )
        if (permissionStep.begin() &&
            ShizukuCommandBridge.requestPermission(activity, SHIZUKU_PERMISSION_REQUEST)
        ) {
            PhoneControlLog.i(
                TAG,
                "authority_setup_dispatch provider=shizuku accepted=true " +
                    "action=${attempt.action.wireName} trigger=$trigger",
            )
            return
        }
        permissionStep.finish()
        PhoneControlLog.w(
            TAG,
            "authority_setup_dispatch provider=shizuku accepted=false " +
                "action=${attempt.action.wireName}",
        )
        leavePending(attempt.condition)
    }

    private fun openExternalStep(
        condition: ShizukuBridgeCondition,
        attempt: PhoneControlShizukuSetupAttempt,
        trigger: String,
    ) {
        when (
            phoneControlShizukuRepeatDisposition(
                attempt,
                lastAttempt,
                externalStepActive,
            )
        ) {
            PhoneControlShizukuRepeatDisposition.WAIT_FOR_EVENT -> {
                PhoneControlLog.i(
                    TAG,
                    "authority_setup_waiting provider=shizuku " +
                        "condition=${condition.wireName}",
                )
                return
            }
            PhoneControlShizukuRepeatDisposition.LEAVE_SELECTED_PENDING -> {
                leavePending(condition)
                return
            }
            PhoneControlShizukuRepeatDisposition.DISPATCH -> Unit
        }
        lastAttempt = attempt
        activity.showPhoneControlToast(condition.toastResource())
        val intent = when (attempt.action) {
            PhoneControlShizukuSetupAction.OPEN_MANAGER -> shizukuManagerIntent()
            PhoneControlShizukuSetupAction.OPEN_STORE -> shizukuStoreIntent()
            else -> error("action does not own an external Shizuku step")
        }
        if (!externalStep.begin()) {
            PhoneControlLog.w(
                TAG,
                "authority_setup_dispatch provider=shizuku accepted=false " +
                    "action=${attempt.action.wireName} reason=step_busy",
            )
            return
        }
        externalStepActive = true
        runCatching { launchExternal(intent) }
            .onSuccess {
                reportGuidance(
                    condition.messageResource(),
                    requestAutomation = true,
                    captureHandoffAfterAutomation =
                        attempt.action == PhoneControlShizukuSetupAction.OPEN_MANAGER,
                )
                PhoneControlLog.i(
                    TAG,
                    "authority_setup_dispatch provider=shizuku accepted=true " +
                        "action=${attempt.action.wireName} trigger=$trigger",
                )
            }
            .onFailure {
                retireExternalStep()
                PhoneControlLog.w(
                    TAG,
                    "authority_setup_dispatch provider=shizuku accepted=false " +
                        "action=${attempt.action.wireName} reason=launch_failed",
                )
                leavePending(condition)
            }
    }

    private fun leavePending(condition: ShizukuBridgeCondition) {
        reportGuidance(
            R.string.phone_control_shizuku_still_needs_user_step,
            requestAutomation = false,
            captureHandoffAfterAutomation = false,
        )
        activity.showPhoneControlToast(R.string.phone_control_shizuku_pending_toast)
        PhoneControlLog.i(
            TAG,
            "authority_setup_result provider=shizuku ready=false pending=true " +
                "condition=${condition.wireName}",
        )
        finishActivity()
    }

    private fun cancelForAuthorityChange() {
        if (closed || activity.isFinishing) return
        retireExternalStep()
        permissionStep.finish()
        PhoneControlService.clearAuthoritySetup(activity, SHIZUKU_PROVIDER_ID)
        PhoneControlLog.i(
            TAG,
            "authority_setup_result provider=shizuku ready=false pending=false " +
                "reason=authority_changed",
        )
        finishActivity()
    }

    private fun reportGuidance(
        messageResource: Int,
        requestAutomation: Boolean,
        captureHandoffAfterAutomation: Boolean,
    ) {
        val guidance = activity.phoneControlString(messageResource)
        PhoneControlSetupNotification.show(
            activity,
            guidance,
            PhoneControlActivity.optionalPowerIntent(
                activity,
                PhoneControlPowerChoice.SHIZUKU,
            ),
        )
        PhoneControlService.reportAuthoritySetup(
            context = activity,
            providerId = SHIZUKU_PROVIDER_ID,
            guidance = guidance,
            requestAutomation = requestAutomation,
            captureHandoffAfterAutomation = captureHandoffAfterAutomation,
        )
    }

    private fun retireExternalStep() {
        if (!externalStepActive) return
        externalStep.finish()
        externalStepActive = false
    }

    private fun shizukuManagerIntent(): Intent =
        activity.packageManager.getLaunchIntentForPackage(SHIZUKU_PACKAGE)
            ?: shizukuStoreIntent()

    private fun shizukuStoreIntent(): Intent {
        val store = Intent(
            Intent.ACTION_VIEW,
            Uri.parse("market://details?id=$SHIZUKU_PACKAGE"),
        ).setPackage(PLAY_STORE_PACKAGE)
        return store.takeIf { it.resolveActivity(activity.packageManager) != null }
            ?: Intent(Intent.ACTION_VIEW, Uri.parse(SHIZUKU_DOWNLOAD_URL))
                .addCategory(Intent.CATEGORY_BROWSABLE)
    }

    private fun ShizukuBridgeCondition.messageResource(): Int = when (this) {
        ShizukuBridgeCondition.SERVICE_STOPPED -> R.string.phone_control_shizuku_start_service
        ShizukuBridgeCondition.PERMISSION_REVOKED ->
            R.string.phone_control_shizuku_restore_permission
        ShizukuBridgeCondition.API_UNSUPPORTED -> R.string.phone_control_shizuku_update
        ShizukuBridgeCondition.PACKAGE_MISSING -> R.string.phone_control_shizuku_install
        ShizukuBridgeCondition.READY,
        ShizukuBridgeCondition.PERMISSION_REQUESTABLE,
        -> error("condition does not own an external Shizuku step")
    }

    private fun ShizukuBridgeCondition.toastResource(): Int = when (this) {
        ShizukuBridgeCondition.SERVICE_STOPPED ->
            R.string.phone_control_shizuku_start_service_toast
        ShizukuBridgeCondition.PERMISSION_REVOKED ->
            R.string.phone_control_shizuku_request_permission_toast
        ShizukuBridgeCondition.API_UNSUPPORTED ->
            R.string.phone_control_shizuku_update_toast
        ShizukuBridgeCondition.PACKAGE_MISSING ->
            R.string.phone_control_shizuku_install_toast
        ShizukuBridgeCondition.READY,
        ShizukuBridgeCondition.PERMISSION_REQUESTABLE,
        -> error("condition does not own an external Shizuku step")
    }

    private fun Bundle.shizukuAttempt(): PhoneControlShizukuSetupAttempt? {
        val condition = getString(STATE_CONDITION)?.let { wireName ->
            ShizukuBridgeCondition.entries.firstOrNull { it.wireName == wireName }
        } ?: return null
        val action = getString(STATE_ACTION)?.let { wireName ->
            PhoneControlShizukuSetupAction.entries.firstOrNull { it.wireName == wireName }
        } ?: return null
        return PhoneControlShizukuSetupAttempt(condition, action)
    }

    private companion object {
        const val TAG = "SGTPhoneControlShizuku"
        const val SHIZUKU_PACKAGE = "moe.shizuku.privileged.api"
        const val SHIZUKU_PROVIDER_ID = "shizuku_shell"
        const val PLAY_STORE_PACKAGE = "com.android.vending"
        const val SHIZUKU_DOWNLOAD_URL = "https://shizuku.rikka.app/download/"
        const val SHIZUKU_PERMISSION_REQUEST = 4082
        const val STATE_CONDITION = "shizuku_condition"
        const val STATE_ACTION = "shizuku_action"
        const val STATE_EXTERNAL_ACTIVE = "shizuku_external_active"
    }
}

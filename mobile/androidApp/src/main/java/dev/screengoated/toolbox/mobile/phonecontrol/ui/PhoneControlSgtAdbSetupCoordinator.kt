package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.content.Intent
import android.provider.Settings
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlService
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlSetupNotification
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PlatformUserStepSlot
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbCommandBridge
import java.io.Closeable
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

internal class PhoneControlSgtAdbSetupCoordinator(
    private val activity: ComponentActivity,
    private val externalStep: PlatformUserStepSlot,
    private val launchExternal: (Intent) -> Unit,
    private val finishActivity: () -> Unit,
) : Closeable {
    private var closed = false
    private var externalStepActive = false
    private var powerObserver: Closeable? = null
    private var reconnectJob: Job? = null

    fun start() {
        powerObserver = PhoneControlPowerPreferences.observe(activity) { choice ->
            if (choice != PhoneControlPowerChoice.SGT_ADB) {
                activity.runOnUiThread(::cancelForAuthorityChange)
            }
        }
        advance("direct")
    }

    fun onExternalReturn() {
        retireExternalStep()
        advance("external_return")
    }

    override fun close() {
        if (closed) return
        closed = true
        reconnectJob?.cancel()
        powerObserver?.close()
        powerObserver = null
    }

    private fun advance(trigger: String) {
        if (closed || activity.isFinishing || activity.isDestroyed) return
        if (PhoneControlPowerPreferences.current(activity) != PhoneControlPowerChoice.SGT_ADB) {
            cancelForAuthorityChange()
            return
        }
        val probe = SgtAdbCommandBridge.probe(activity)
        PhoneControlLog.i(
            TAG,
            "authority_setup_step provider=$PROVIDER_ID trigger=$trigger " +
                "condition=${probe.condition.name.lowercase()} state=${probe.state.wireName}",
        )
        when (probe.state) {
            CapabilityState.READY -> complete()
            CapabilityState.UNSUPPORTED -> leavePending()
            CapabilityState.DEGRADED -> reconnectThenContinue()
            CapabilityState.NEEDS_USER_STEP,
            CapabilityState.REVOKED,
            CapabilityState.UNAVAILABLE,
            -> openWirelessDebugging()
        }
    }

    private fun openWirelessDebugging() {
        if (externalStepActive) return
        val intent = Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS)
        if (intent.resolveActivity(activity.packageManager) == null || !externalStep.begin()) {
            leavePending()
            return
        }
        externalStepActive = true
        Toast.makeText(
            activity,
            R.string.phone_control_sgt_adb_setup,
            Toast.LENGTH_LONG,
        ).show()
        runCatching { launchExternal(intent) }
            .onSuccess {
                reportGuidance(
                    R.string.phone_control_sgt_adb_setup,
                    requestAutomation = true,
                    captureHandoffAfterAutomation = true,
                )
            }
            .onFailure {
                retireExternalStep()
                leavePending()
            }
    }

    private fun reconnectThenContinue() {
        if (reconnectJob?.isActive == true) return
        reportGuidance(
            R.string.phone_control_sgt_adb_pending,
            requestAutomation = false,
            captureHandoffAfterAutomation = false,
        )
        reconnectJob = activity.lifecycleScope.launch {
            val probe = SgtAdbCommandBridge.reconnect(activity)
            if (closed) return@launch
            if (probe.state == CapabilityState.READY) {
                complete()
            } else {
                openWirelessDebugging()
            }
        }
    }

    private fun complete() {
        retireExternalStep()
        val continuation = when {
            PhoneControlService.captureSuspended ->
                PhoneControlActivity.resumeCaptureIntent(activity)
            !PhoneControlService.state.value.running ->
                PhoneControlActivity.activationIntent(activity)
            else -> null
        }
        PhoneControlService.clearAuthoritySetup(activity, PROVIDER_ID)
        Toast.makeText(activity, R.string.phone_control_sgt_adb_ready, Toast.LENGTH_SHORT).show()
        PhoneControlLog.i(TAG, "authority_setup_result provider=$PROVIDER_ID ready=true")
        if (continuation != null) {
            if (activity.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) {
                activity.startActivity(continuation)
            } else {
                PhoneControlSetupNotification.show(
                    activity,
                    activity.getString(R.string.phone_control_sgt_adb_ready_resume),
                    continuation,
                )
            }
        }
        finishActivity()
    }

    private fun leavePending() {
        reportGuidance(
            R.string.phone_control_sgt_adb_pending,
            requestAutomation = false,
            captureHandoffAfterAutomation = false,
        )
        Toast.makeText(
            activity,
            R.string.phone_control_sgt_adb_pending,
            Toast.LENGTH_LONG,
        ).show()
        PhoneControlLog.i(TAG, "authority_setup_result provider=$PROVIDER_ID pending=true")
        finishActivity()
    }

    private fun reportGuidance(
        messageResource: Int,
        requestAutomation: Boolean,
        captureHandoffAfterAutomation: Boolean,
    ) {
        val guidance = activity.getString(messageResource)
        PhoneControlSetupNotification.show(
            activity,
            guidance,
            PhoneControlActivity.optionalPowerIntent(activity, PhoneControlPowerChoice.SGT_ADB),
        )
        PhoneControlService.reportAuthoritySetup(
            context = activity,
            providerId = PROVIDER_ID,
            guidance = guidance,
            requestAutomation = requestAutomation,
            captureHandoffAfterAutomation = captureHandoffAfterAutomation,
        )
    }

    private fun cancelForAuthorityChange() {
        if (closed || activity.isFinishing) return
        retireExternalStep()
        PhoneControlService.clearAuthoritySetup(activity, PROVIDER_ID)
        finishActivity()
    }

    private fun retireExternalStep() {
        if (!externalStepActive) return
        externalStep.finish()
        externalStepActive = false
    }

    private companion object {
        const val TAG = "SGTPhoneControlAdbSetup"
        const val PROVIDER_ID = "sgt_adb_bridge"
    }
}

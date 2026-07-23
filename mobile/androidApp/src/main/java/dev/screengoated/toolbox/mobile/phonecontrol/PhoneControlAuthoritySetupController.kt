package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Context
import android.content.Intent
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntime
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlUiGoalCompletion
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlUiGoalOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerChoice
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences

internal class PhoneControlAuthoritySetupController(
    private val context: Context,
    private val runtime: () -> PhoneControlRuntime?,
    private val publishGuidance: (String) -> Unit,
    private val enterProtectedCheckpoint: (String) -> Boolean,
) {
    var guidance: String = ""
        private set

    private var providerId: String? = null
    private var captureHandoffGoalId: Long? = null
    private var captureHandoffProviderId: String? = null

    fun update(intent: Intent) {
        val requestedProvider = intent.getStringExtra(EXTRA_AUTHORITY_PROVIDER_ID).orEmpty()
        val selected = PhoneControlPowerPreferences.current(context)
        if (requestedProvider.isBlank() || selected?.elevatedProviderId != requestedProvider) {
            PhoneControlLog.w(
                TAG,
                "authority_setup_progress accepted=false reason=provider_not_selected",
            )
            return
        }
        guidance = intent.getStringExtra(EXTRA_AUTHORITY_GUIDANCE).orEmpty().trim()
        providerId = requestedProvider
        publishGuidance(guidance)
        val automationRequested = intent.getBooleanExtra(
            EXTRA_AUTHORITY_AUTOMATION_REQUESTED,
            false,
        )
        val goalId = if (automationRequested) {
            runtime()?.submitUserInterfaceGoal(authoritySetupGoal(requestedProvider))
        } else {
            null
        }
        val handoffRequested = intent.getBooleanExtra(
            EXTRA_CAPTURE_HANDOFF_AFTER_AUTOMATION,
            false,
        )
        if (handoffRequested && goalId != null) {
            captureHandoffGoalId = goalId
            captureHandoffProviderId = requestedProvider
        }
        PhoneControlLog.i(
            TAG,
            "authority_setup_progress accepted=true provider=$requestedProvider " +
                "automation_requested=$automationRequested automation_goal=${goalId ?: "none"} " +
                "capture_handoff=${handoffRequested && goalId != null}",
        )
    }

    fun clear(requestedProvider: String? = null, reason: String? = null) {
        guidance = ""
        providerId = null
        captureHandoffGoalId = null
        captureHandoffProviderId = null
        PhoneControlSetupNotification.clear(context)
        publishGuidance("")
        PhoneControlLog.i(
            TAG,
            "authority_setup_clear accepted=true provider=${requestedProvider.orEmpty().ifBlank {
                "none"
            }}${reason?.let { " reason=$it" }.orEmpty()}",
        )
    }

    fun onPowerChoiceSelected(choice: PhoneControlPowerChoice) {
        if (providerId == null || providerId == choice.elevatedProviderId) return
        clear(reason = "authority_changed")
    }

    fun onUserInterfaceGoalFinished(completion: PhoneControlUiGoalCompletion) {
        if (captureHandoffGoalId != completion.id) return
        val requestedProvider = captureHandoffProviderId
        captureHandoffGoalId = null
        captureHandoffProviderId = null
        if (completion.outcome != PhoneControlUiGoalOutcome.COMPLETED) {
            PhoneControlLog.i(
                TAG,
                "protected_checkpoint_handoff accepted=false reason=goal_interrupted " +
                    "provider=${requestedProvider ?: "none"}",
            )
            return
        }
        if (requestedProvider.isNullOrBlank() ||
            PhoneControlPowerPreferences.current(context)?.elevatedProviderId != requestedProvider
        ) {
            PhoneControlLog.w(
                TAG,
                "protected_checkpoint_handoff accepted=false reason=provider_not_selected",
            )
            return
        }
        val accepted = enterProtectedCheckpoint(requestedProvider)
        PhoneControlLog.i(
            TAG,
            "protected_checkpoint_handoff accepted=$accepted provider=$requestedProvider",
        )
    }

    fun resumeSelectedAuthoritySetup(onResume: (PhoneControlPowerChoice) -> Unit): Boolean {
        val selected = PhoneControlPowerPreferences.current(context)
            ?.takeIf { it.elevatedProviderId != null }
            ?: return false
        val provider = selected.elevatedProviderId
            ?.let(PrivilegedCommandProviderRegistry::find)
            ?: return false
        if (provider.probe(context).state == CapabilityState.READY) return false
        onResume(selected)
        return true
    }

    private companion object {
        const val TAG = "SGTPhoneControlService"
    }
}

internal const val EXTRA_AUTHORITY_PROVIDER_ID =
    "dev.screengoated.toolbox.mobile.phonecontrol.AUTHORITY_PROVIDER_ID"
internal const val EXTRA_AUTHORITY_GUIDANCE =
    "dev.screengoated.toolbox.mobile.phonecontrol.AUTHORITY_GUIDANCE"
internal const val EXTRA_AUTHORITY_AUTOMATION_REQUESTED =
    "dev.screengoated.toolbox.mobile.phonecontrol.AUTHORITY_AUTOMATION_REQUESTED"
internal const val EXTRA_CAPTURE_HANDOFF_AFTER_AUTOMATION =
    "dev.screengoated.toolbox.mobile.phonecontrol.CAPTURE_HANDOFF_AFTER_AUTOMATION"

private fun authoritySetupGoal(providerId: String): String =
    "The user selected Phone Control authority provider `$providerId` and expects setup to " +
        "continue. Use the normal full tool catalog on the current visible Android setup " +
        "surface. Automate reversible navigation and diagnosis to the exact user-owned " +
        "checkpoint surface. Opening that surface is allowed. Stop before reading, filling, " +
        "submitting, or approving protected content or an OS-owned confirmation. If screen " +
        "sharing hides the checkpoint, leave the nearest relevant surface visible and finish " +
        "the bounded goal. Do not claim the provider is ready until a fresh probe says so."

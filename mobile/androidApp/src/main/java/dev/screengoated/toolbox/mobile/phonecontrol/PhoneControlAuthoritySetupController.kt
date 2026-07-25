package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntime
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlUiGoalCompletion
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlUiGoalOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlUiGoalPresentation
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerChoice
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences

internal enum class PhoneControlAuthorityResumeDisposition {
    NONE,
    READY,
    RESUME_SETUP,
}

internal fun phoneControlAuthorityResumeDisposition(
    selected: PhoneControlPowerChoice?,
    providerState: CapabilityState?,
): PhoneControlAuthorityResumeDisposition = when {
    selected?.elevatedProviderId == null || providerState == null ->
        PhoneControlAuthorityResumeDisposition.NONE
    providerState == CapabilityState.READY -> PhoneControlAuthorityResumeDisposition.READY
    else -> PhoneControlAuthorityResumeDisposition.RESUME_SETUP
}

internal class PhoneControlAuthoritySetupController(
    private val context: Context,
    private val runtime: () -> PhoneControlRuntime?,
    private val publishGuidance: (String) -> Unit,
    private val enterProtectedCheckpoint: (String) -> Boolean,
) {
    private val mainHandler = Handler(Looper.getMainLooper())
    private val automationOwner = PhoneControlAuthorityAutomationOwner()

    var guidance: String = ""
        private set

    private var providerId: String? = null
    private var readyFeedbackGeneration = 0L

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
        if (PhoneControlProtectedCheckpointRegistry.hasActiveCheckpoint()) {
            PhoneControlLog.w(
                TAG,
                "authority_setup_progress accepted=false reason=protected_checkpoint_active",
            )
            return
        }
        readyFeedbackGeneration += 1
        guidance = intent.getStringExtra(EXTRA_AUTHORITY_GUIDANCE).orEmpty().trim()
        providerId = requestedProvider
        publishGuidance(guidance)
        val automationRequested = intent.getBooleanExtra(
            EXTRA_AUTHORITY_AUTOMATION_REQUESTED,
            false,
        )
        val handoffRequested = intent.getBooleanExtra(
            EXTRA_CAPTURE_HANDOFF_AFTER_AUTOMATION,
            false,
        )
        val automationDisposition = automationOwner.disposition(
            automationRequested = automationRequested,
            requestedProvider = requestedProvider,
        )
        val ownership = when (automationDisposition) {
            PhoneControlAuthorityAutomationDisposition.NONE -> null
            PhoneControlAuthorityAutomationDisposition.SUBMIT -> runtime()
                ?.submitUserInterfaceGoal(
                    authoritySetupGoal(requestedProvider),
                    PhoneControlUiGoalPresentation.SILENT,
                )
                ?.let { submittedId ->
                    automationOwner.begin(
                        goalId = submittedId,
                        providerId = requestedProvider,
                        captureHandoff = handoffRequested,
                    )
                }
            PhoneControlAuthorityAutomationDisposition.COALESCE ->
                automationOwner.coalesce(requestedProvider, handoffRequested)
            PhoneControlAuthorityAutomationDisposition.BLOCKED -> null
        }
        val automationAccepted =
            automationDisposition != PhoneControlAuthorityAutomationDisposition.BLOCKED
        PhoneControlLog.i(
            TAG,
            "authority_setup_progress accepted=$automationAccepted " +
                "provider=$requestedProvider " +
                "automation_requested=$automationRequested " +
                "automation_goal=${ownership?.goalId ?: "none"} " +
                "automation_disposition=${automationDisposition.name.lowercase()} " +
                "capture_handoff=${ownership?.captureHandoff == true}",
        )
    }

    fun clear(requestedProvider: String? = null, reason: String? = null) {
        if (!requestedProvider.isNullOrBlank() &&
            providerId != null &&
            providerId != requestedProvider
        ) {
            PhoneControlLog.w(
                TAG,
                "authority_setup_clear accepted=false provider=$requestedProvider " +
                    "reason=active_provider_mismatch",
            )
            return
        }
        readyFeedbackGeneration += 1
        guidance = ""
        providerId = null
        automationOwner.clear()
        PhoneControlSetupNotification.clear(context)
        publishGuidance("")
        PhoneControlLog.i(
            TAG,
            "authority_setup_clear accepted=true provider=${requestedProvider.orEmpty().ifBlank {
                "none"
            }}${reason?.let { " reason=$it" }.orEmpty()}",
        )
    }

    fun replaceGuidance(requestedProvider: String, nextGuidance: String): Boolean {
        val selectedProvider = PhoneControlPowerPreferences.current(context)?.elevatedProviderId
        val normalized = nextGuidance.trim()
        if (requestedProvider.isBlank() ||
            normalized.isBlank() ||
            selectedProvider != requestedProvider ||
            (providerId != null && providerId != requestedProvider)
        ) {
            PhoneControlLog.w(
                TAG,
                "authority_setup_guidance accepted=false reason=provider_or_state_mismatch",
            )
            return false
        }
        readyFeedbackGeneration += 1
        guidance = normalized
        providerId = requestedProvider
        automationOwner.clear()
        publishGuidance(normalized)
        PhoneControlLog.i(
            TAG,
            "authority_setup_guidance accepted=true provider=$requestedProvider " +
                "state=setup_transition",
        )
        return true
    }

    fun onPowerChoiceSelected(choice: PhoneControlPowerChoice) {
        if (providerId == null || providerId == choice.elevatedProviderId) return
        clear(reason = "authority_changed")
    }

    fun onUserInterfaceGoalFinished(completion: PhoneControlUiGoalCompletion) {
        val ownership = automationOwner.complete(completion.id) ?: return
        if (!ownership.captureHandoff) return
        val requestedProvider = ownership.providerId
        if (completion.outcome != PhoneControlUiGoalOutcome.COMPLETED) {
            PhoneControlLog.i(
                TAG,
                "protected_checkpoint_handoff accepted=false reason=goal_interrupted " +
                    "provider=$requestedProvider",
            )
            return
        }
        if (PhoneControlPowerPreferences.current(context)?.elevatedProviderId !=
            requestedProvider
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

    fun resumeSelectedAuthoritySetup(
        announceReady: Boolean = false,
        onResume: (PhoneControlPowerChoice) -> Unit,
    ): Boolean {
        val selected = PhoneControlPowerPreferences.current(context)
            ?.takeIf { it.elevatedProviderId != null }
            ?: return false
        val provider = selected.elevatedProviderId
            ?.let(PrivilegedCommandProviderRegistry::find)
            ?: return false
        return when (
            phoneControlAuthorityResumeDisposition(selected, provider.probe(context).state)
        ) {
            PhoneControlAuthorityResumeDisposition.NONE -> false
            PhoneControlAuthorityResumeDisposition.READY -> {
                if (announceReady) publishVerifiedReady(selected)
                false
            }
            PhoneControlAuthorityResumeDisposition.RESUME_SETUP -> {
                onResume(selected)
                true
            }
        }
    }

    private fun publishVerifiedReady(choice: PhoneControlPowerChoice) {
        val selectedProvider = choice.elevatedProviderId ?: return
        val providerLabel = context.phoneControlString(choice.labelResource())
        val message = context.phoneControlString(
            R.string.phone_control_authority_ready,
            providerLabel,
        )
        val generation = ++readyFeedbackGeneration
        guidance = message
        providerId = selectedProvider
        PhoneControlSetupNotification.clear(context)
        publishGuidance(message)
        context.showPhoneControlToast(
            R.string.phone_control_authority_ready_toast,
            providerLabel,
        )
        PhoneControlLog.i(
            TAG,
            "authority_setup_result provider=$selectedProvider ready=true feedback=local",
        )
        mainHandler.postDelayed({
            if (readyFeedbackGeneration == generation && providerId == selectedProvider) {
                clear(selectedProvider, reason = "ready_visual_complete")
            }
        }, READY_VISUAL_DURATION_MS)
    }

    private fun PhoneControlPowerChoice.labelResource(): Int = when (this) {
        PhoneControlPowerChoice.STANDARD -> R.string.phone_control_power_standard
        PhoneControlPowerChoice.SGT_ADB -> R.string.phone_control_power_sgt_adb
        PhoneControlPowerChoice.SHIZUKU -> R.string.phone_control_power_shizuku
        PhoneControlPowerChoice.ROOT -> R.string.phone_control_power_root
    }

    private companion object {
        const val TAG = "SGTPhoneControlService"
        const val READY_VISUAL_DURATION_MS = 4_000L
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
        "continue as a silent app-owned automation turn. Do not narrate progress or results. " +
        "Use the normal full tool catalog on the current visible Android setup " +
        "surface. Automate reversible navigation and diagnosis to the exact user-owned " +
        "checkpoint surface. Opening that surface is allowed. Stop before reading, filling, " +
        "submitting, or approving protected content or an OS-owned confirmation. If screen " +
        "sharing hides the checkpoint, leave the nearest relevant surface visible and finish " +
        "the bounded goal. Do not claim the provider is ready until a fresh probe says so."

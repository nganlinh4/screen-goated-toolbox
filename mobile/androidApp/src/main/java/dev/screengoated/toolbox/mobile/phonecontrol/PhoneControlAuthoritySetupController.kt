package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupNavigationContract
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntime
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlUiGoalCompletion
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlUiGoalPresentation
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerChoice
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences

internal enum class PhoneControlAuthorityResumeDisposition {
    NONE,
    READY,
    RESUME_SETUP,
}

internal enum class PhoneControlAuthoritySetupSessionEvent {
    STARTED,
    SUCCEEDED,
    ENDED,
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
    private val navigationContract: (String) -> PhoneControlProtectedSetupNavigationContract?,
    private val protectedHandoff: PhoneControlProtectedCheckpointHandoff,
    private val onSetupSessionEvent: (PhoneControlAuthoritySetupSessionEvent) -> Unit = {},
) {
    private val mainHandler = Handler(Looper.getMainLooper())
    private val automationOwner = PhoneControlAuthorityAutomationOwner()

    var guidance: String = ""
        private set

    private var providerId: String? = null
    private var readyFeedbackGeneration = 0L
    private var setupSessionActive = false

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
        if (!setupSessionActive) {
            setupSessionActive = true
            onSetupSessionEvent(PhoneControlAuthoritySetupSessionEvent.STARTED)
        }
        readyFeedbackGeneration += 1
        guidance = intent.getStringExtra(EXTRA_AUTHORITY_GUIDANCE).orEmpty().trim()
        providerId = requestedProvider
        publishGuidance(guidance)
        val automationRequested = intent.getBooleanExtra(
            EXTRA_AUTHORITY_AUTOMATION_REQUESTED,
            false,
        )
        val checkpointMonitoringRequested = intent.getBooleanExtra(
            EXTRA_PROTECTED_CHECKPOINT_MONITORING,
            false,
        )
        val setupContract = navigationContract(requestedProvider)
        val automationDisposition = automationOwner.disposition(
            automationRequested = automationRequested,
            requestedProvider = requestedProvider,
        )
        val ownership = when (automationDisposition) {
            PhoneControlAuthorityAutomationDisposition.NONE -> null
            PhoneControlAuthorityAutomationDisposition.SUBMIT -> runtime()
                ?.submitUserInterfaceGoal(
                    authoritySetupGoal(
                        requestedProvider,
                        setupContract.takeIf { checkpointMonitoringRequested },
                    ),
                    PhoneControlUiGoalPresentation.SILENT,
                )
                ?.let { submittedId ->
                    automationOwner.begin(
                        goalId = submittedId,
                        providerId = requestedProvider,
                        checkpointMonitoring = checkpointMonitoringRequested,
                    )
                }
            PhoneControlAuthorityAutomationDisposition.COALESCE ->
                automationOwner.coalesce(requestedProvider, checkpointMonitoringRequested)
            PhoneControlAuthorityAutomationDisposition.BLOCKED -> null
        }
        val automationAccepted = when (automationDisposition) {
            PhoneControlAuthorityAutomationDisposition.NONE -> true
            PhoneControlAuthorityAutomationDisposition.SUBMIT,
            PhoneControlAuthorityAutomationDisposition.COALESCE,
            -> ownership != null
            PhoneControlAuthorityAutomationDisposition.BLOCKED -> false
        }
        val monitoringAccepted = ownership?.takeIf { it.checkpointMonitoring }?.let {
            protectedHandoff.arm(requestedProvider, it.goalId)
        } ?: !checkpointMonitoringRequested
        if (!monitoringAccepted) {
            ownership?.goalId?.let { runtime()?.requestProtectedCheckpointBoundary(it) }
        }
        PhoneControlLog.i(
            TAG,
            "authority_setup_progress accepted=${automationAccepted && monitoringAccepted} " +
                "provider=$requestedProvider " +
                "automation_requested=$automationRequested " +
                "automation_goal=${ownership?.goalId ?: "none"} " +
                "automation_disposition=${automationDisposition.name.lowercase()} " +
                "checkpoint_monitoring=${ownership?.checkpointMonitoring == true}",
        )
    }

    fun clear(requestedProvider: String? = null, reason: String? = null) {
        clearInternal(requestedProvider, reason, endSetupSession = true)
    }

    fun clearForReadyProbe(reason: String) {
        clearInternal(requestedProvider = null, reason = reason, endSetupSession = false)
    }

    fun completeIfVerifiedReady(requestedProvider: String) {
        if (!setupSessionActive) {
            PhoneControlLog.i(
                TAG,
                "authority_setup_result provider=$requestedProvider ready=true feedback=already_published",
            )
            return
        }
        val selected = PhoneControlPowerPreferences.current(context)
        val provider = PrivilegedCommandProviderRegistry.find(requestedProvider)
        val ready = selected?.elevatedProviderId == requestedProvider &&
            provider?.probe(context)?.state == CapabilityState.READY
        if (ready) {
            publishVerifiedReady(selected)
        } else {
            clear(requestedProvider, reason = "ready_verification_failed")
            PhoneControlLog.w(
                TAG,
                "authority_setup_result provider=$requestedProvider ready=false " +
                    "reason=fresh_probe_not_ready",
            )
        }
    }

    private fun clearInternal(
        requestedProvider: String?,
        reason: String?,
        endSetupSession: Boolean,
    ) {
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
        protectedHandoff.cancel(providerId, reason ?: "authority_setup_clear")
        guidance = ""
        providerId = null
        automationOwner.clear()?.goalId?.let { runtime()?.requestProtectedCheckpointBoundary(it) }
        PhoneControlSetupNotification.clear(context)
        publishGuidance("")
        if (endSetupSession) {
            finishSetupSession(PhoneControlAuthoritySetupSessionEvent.ENDED)
        }
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
        automationOwner.clear()?.goalId?.let { runtime()?.requestProtectedCheckpointBoundary(it) }
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
        if (ownership.checkpointMonitoring) {
            protectedHandoff.navigationFinished(ownership.providerId, completion)
        }
        PhoneControlLog.i(
            TAG,
            "ui_goal_finished provider=${ownership.providerId} " +
                "outcome=${completion.outcome.name.lowercase()} " +
                "checkpoint_monitoring=${ownership.checkpointMonitoring}",
        )
    }

    fun continueProtectedNavigation(
        requestedProvider: String,
        previousGoalId: Long,
        reason: String,
    ): Long? {
        val selectedProvider = PhoneControlPowerPreferences.current(context)?.elevatedProviderId
        val candidate = runtime()
        val contract = navigationContract(requestedProvider)
        if (requestedProvider.isBlank() ||
            providerId != requestedProvider ||
            selectedProvider != requestedProvider ||
            candidate == null ||
            contract == null ||
            automationOwner.disposition(true, requestedProvider) !=
            PhoneControlAuthorityAutomationDisposition.SUBMIT
        ) {
            PhoneControlLog.w(
                TAG,
                "authority_setup_navigation_retry accepted=false provider=$requestedProvider " +
                    "goal_id=$previousGoalId reason=state_changed",
            )
            return null
        }
        val nextGoalId = candidate.submitUserInterfaceGoal(
            authoritySetupGoal(requestedProvider, contract, reason),
            PhoneControlUiGoalPresentation.SILENT,
        ) ?: return null
        automationOwner.begin(
            goalId = nextGoalId,
            providerId = requestedProvider,
            checkpointMonitoring = true,
        )
        PhoneControlLog.i(
            TAG,
            "authority_setup_navigation_retry accepted=true provider=$requestedProvider " +
                "goal_id=$nextGoalId reason=$reason",
        )
        return nextGoalId
    }

    fun onProtectedNavigationExhausted(requestedProvider: String, reason: String) {
        if (providerId != requestedProvider) return
        automationOwner.clear()?.goalId?.let { runtime()?.requestProtectedCheckpointBoundary(it) }
        readyFeedbackGeneration += 1
        guidance = ""
        PhoneControlSetupNotification.clear(context)
        publishGuidance("")
        finishSetupSession(PhoneControlAuthoritySetupSessionEvent.ENDED)
        PhoneControlLog.w(
            TAG,
            "authority_setup_result provider=$requestedProvider pending=true reason=$reason",
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
        finishSetupSession(PhoneControlAuthoritySetupSessionEvent.SUCCEEDED)
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

    private fun finishSetupSession(event: PhoneControlAuthoritySetupSessionEvent) {
        if (!setupSessionActive) return
        setupSessionActive = false
        onSetupSessionEvent(event)
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
internal const val EXTRA_PROTECTED_CHECKPOINT_MONITORING =
    "dev.screengoated.toolbox.mobile.phonecontrol.PROTECTED_CHECKPOINT_MONITORING"

private fun authoritySetupGoal(
    providerId: String,
    contract: PhoneControlProtectedSetupNavigationContract?,
    continuationReason: String? = null,
): String =
    "The user selected Phone Control authority provider `$providerId` and expects setup to " +
        "continue as a silent app-owned automation turn. Do not narrate progress or results. " +
        contract?.let {
            "The provider setup contract requires ${it.platformCapability}. Navigate to " +
                "${it.destinationState}. "
        }.orEmpty() +
        continuationReason?.let {
            "A previous navigation generation ended, but the local structural verifier " +
                "proved that the required checkpoint was absent ($it). Continue from a " +
                "fresh observation of the current surface and choose a different next action " +
                "when prior evidence did not establish progress. "
        }.orEmpty() +
        "Use the normal full tool catalog on the current visible Android setup " +
        "surface. Automate reversible navigation and diagnosis to the exact user-owned " +
        "checkpoint surface. A parent settings page, search result, or unchanged screen is " +
        "not completion. Opening the exact checkpoint surface is allowed. Stop before reading, " +
        "filling, " +
        "submitting, or approving protected content or an OS-owned confirmation. The local " +
        "checkpoint monitor owns protected-value handling. Do not claim the provider is ready " +
        "until a fresh probe says so."

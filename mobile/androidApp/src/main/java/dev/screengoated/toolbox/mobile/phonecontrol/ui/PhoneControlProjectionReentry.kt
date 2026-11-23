package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.content.Intent
import androidx.activity.result.ActivityResult
import androidx.lifecycle.lifecycleScope
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.awaitSafeProjectionReplay
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlSetupNotification
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlProjectionReceipt
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlProjectionDispatches
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlService
import dev.screengoated.toolbox.mobile.phonecontrol.EXTRA_ORPHAN_PROJECTION_REENTRY
import dev.screengoated.toolbox.mobile.phonecontrol.OrphanProjectionReentry
import dev.screengoated.toolbox.mobile.phonecontrol.DetachedProjectionReentry
import dev.screengoated.toolbox.mobile.phonecontrol.canDeliverFreshProjectionConsent
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.phoneControlString
import dev.screengoated.toolbox.mobile.phonecontrol.showPhoneControlToast
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionGrant
import kotlinx.coroutines.launch

internal enum class PhoneControlCoordinatorMode(val wireName: String) {
    ACTIVATE("activate"),
    SGT_ADB("sgt_adb"),
    SHIZUKU("shizuku"),
    ROOT("root"),
    SGT_ADB_FORGET("sgt_adb_forget"),
    RESUME_CAPTURE("resume_capture"),
    RETURN_TO_APP("return_to_app"),
    CANCEL_SETUP("cancel_setup"),
}

internal fun phoneControlCoordinatorMode(value: String?): PhoneControlCoordinatorMode =
    PhoneControlCoordinatorMode.entries.firstOrNull { it.wireName == value }
        ?: PhoneControlCoordinatorMode.ACTIVATE

internal fun PhoneControlActivity.handleProjectionResult(result: ActivityResult) {
    userSteps.projection.finish()
    if (userSteps.projectionRequest == null) return
    userSteps.pendingProjectionResult = result
    userSteps.projectionSettling = true
    resumeProjectionResultAfterTeardown()
}

internal fun PhoneControlActivity.resumeProjectionResultAfterTeardown() {
    if (projectionResultJob?.isActive == true) return
    val result = userSteps.pendingProjectionResult ?: return
    val request = userSteps.projectionRequest ?: return
    projectionResultJob = lifecycleScope.launch {
        PhoneControlProjectionDispatches.awaitTeardown()
        if (userSteps.pendingProjectionResult !== result || userSteps.projectionRequest !== request) return@launch
        userSteps.pendingProjectionResult = null
        userSteps.projectionRequest = null
        userSteps.projectionSettling = false
        completeProjectionResult(request, result)
    }
}

private fun PhoneControlActivity.completeProjectionResult(
    request: PhoneControlProjectionRequest,
    result: ActivityResult,
) {
    val grant = PhoneControlProjectionGrant.fromActivityResult(result.resultCode, result.data)
    PhoneControlLog.i(
        PROJECTION_TAG,
        "activation_user_step_returned step=media_projection " +
            "surface=system_capture_dialog accepted=${grant != null}",
    )
    if (request is PhoneControlProjectionRequest.Protected) {
        if (!canDeliverFreshProjectionConsent(request.owner, request.generation, resumeMode = true)) {
            retireStaleProjectionResult(request.generation, request.sequence, grant != null)
            return
        }
        awaitingStep = null
        val dispatch = grant?.let {
            PhoneControlService.attachProjection(this, it, request.owner, request.generation)
        }
        userSteps.projectionDispatch = dispatch
        val accepted = dispatch != null
        if (!accepted) PhoneControlProjectionDispatches.releaseConsent(request.generation)
        PhoneControlLog.i(PROJECTION_TAG, "capture_resume_result accepted=$accepted runtime_reused=true")
        if (!accepted) {
            showPhoneControlToast(R.string.phone_control_activation_projection_toast)
            PhoneControlSetupNotification.show(
                this,
                phoneControlString(R.string.phone_control_activation_projection_needed),
                PhoneControlActivity.resumeCaptureIntent(this),
            )
        }
        finishAndReplayProjectionReentry(request.owner.takeIf { accepted })
        return
    }
    if (request !is PhoneControlProjectionRequest.Activation) return
    if (!PhoneControlProjectionDispatches.ownsConsent(request.generation)) {
        retireStaleProjectionResult(request.generation, request.sequence, grant != null)
        return
    }
    userSteps.projectionSettling = true
    awaitingStep = PhoneControlActivationStep.MEDIA_PROJECTION
    if (grant == null) {
        PhoneControlProjectionDispatches.releaseConsent(request.generation)
        awaitingStep = null
        projectionGrant = null
        userSteps.projectionGrantGeneration = null
        abortActivation(PhoneControlActivationStep.MEDIA_PROJECTION)
    } else {
        projectionGrant = grant
        userSteps.projectionGrantGeneration = request.generation
        completeActivationStep()
    }
}

private fun PhoneControlActivity.retireStaleProjectionResult(generation: String, requestSequence: Long, accepted: Boolean) {
    PhoneControlProjectionDispatches.releaseConsent(generation)
    val next = userSteps.deferredReentry ?: if (accepted) {
        if (PhoneControlService.captureSuspended) PhoneControlActivity.resumeCaptureIntent(this)
        else PhoneControlActivity.activationIntent(this)
    } else null
    val sequence = userSteps.deferredReentrySequence ?: requestSequence.takeIf { accepted }
    if (next != null && sequence != null) {
        when (PhoneControlProjectionDispatches.preserveDetachedReentry(sequence, next) { guarded ->
            PhoneControlSetupNotification.showOrphanedReentry(
                applicationContext,
                phoneControlString(R.string.phone_control_private_setup_pending),
                guarded,
            )
        }) {
            DetachedProjectionReentry.RETAINED -> {
                PhoneControlSetupNotification.clearOrphanedReentry(this)
                showPhoneControlToast(R.string.phone_control_projection_busy)
            }
            DetachedProjectionReentry.NOTIFIED, DetachedProjectionReentry.SUPERSEDED -> Unit
        }
    }
    userSteps.deferredReentry = null
    userSteps.projectionSettling = false
    userSteps.settlingProjectionOwner = null
    userSteps.projectionDispatch = null
    finish()
}

internal fun PhoneControlActivity.showProjectionBusy(retry: Intent) {
    showProjectionRetry(retry, R.string.phone_control_projection_busy)
}

private fun PhoneControlActivity.showProjectionRetry(retry: Intent, message: Int) {
    showPhoneControlToast(message)
    runCatching {
        PhoneControlSetupNotification.showProjectionBusy(this, phoneControlString(message), retry)
    }
}

internal fun PhoneControlActivity.acceptOrSupersedeOrphanReentry(
    intent: Intent,
    freshIntent: Boolean = true,
): Boolean {
    if (!freshIntent && !intent.hasExtra(EXTRA_ORPHAN_PROJECTION_REENTRY)) return true
    if (PhoneControlCoordinatorReentryLauncher.isCoordinatorIntent(intent) &&
        !intent.hasExtra(EXTRA_ORPHAN_PROJECTION_REENTRY)
    ) return true
    val generation = if (intent.hasExtra(EXTRA_ORPHAN_PROJECTION_REENTRY)) {
        intent.getLongExtra(EXTRA_ORPHAN_PROJECTION_REENTRY, 0L)
    } else {
        null
    }
    return when (PhoneControlProjectionDispatches.inspectOrphanReentry(generation)) {
        OrphanProjectionReentry.STALE -> false
        OrphanProjectionReentry.STALE_UNOWNED -> {
            PhoneControlSetupNotification.clearOrphanedReentry(this)
            false
        }
        OrphanProjectionReentry.ACCEPTED, OrphanProjectionReentry.SUPERSEDED -> {
            PhoneControlSetupNotification.clearOrphanedReentry(this)
            intent.removeExtra(EXTRA_ORPHAN_PROJECTION_REENTRY)
            if (generation != null) PhoneControlCoordinatorReentryLauncher.clearCoordinatorTag(intent)
            true
        }
        OrphanProjectionReentry.NONE -> {
            if (freshIntent) PhoneControlSetupNotification.clearOrphanedReentry(this)
            true
        }
    }
}

internal fun PhoneControlActivity.finishAndReplayProjectionReentry(owner: String? = null) {
    if (userSteps.deferredReentry == null && userSteps.projectionDispatch == null &&
        !PhoneControlProjectionDispatches.hasDetachedReentry()
    ) {
        userSteps.projectionSettling = false
        userSteps.settlingProjectionOwner = null
        userSteps.projectionDispatch = null
        finish()
        return
    }
    userSteps.projectionSettling = true
    userSteps.settlingProjectionOwner = owner
    val activeDispatch = userSteps.projectionDispatch
    val pendingReentry = userSteps.deferredReentry
    var transferred = true
    if (activeDispatch != null && pendingReentry != null) {
        transferred = PhoneControlProjectionDispatches.stageReentry(
            activeDispatch, pendingReentry,
            replaceExisting = !PhoneControlCoordinatorReentryLauncher.isCoordinatorIntent(pendingReentry),
        )
        if (transferred) {
            userSteps.deferredReentry = null
        }
    }
    lifecycleScope.launch {
        val dispatch = activeDispatch
        val receipt = dispatch?.let { awaitSafeProjectionReplay(it) }
        PhoneControlProjectionDispatches.awaitTeardown()
        val replay = dispatch?.let { PhoneControlProjectionDispatches.claimReplay(it) }
        val next = if (dispatch == null || !transferred) {
            userSteps.deferredReentry ?: PhoneControlProjectionDispatches.takeDetachedReentry()
        } else {
            replay?.intent
        }
        userSteps.deferredReentry = null
        userSteps.projectionSettling = false
        userSteps.settlingProjectionOwner = null
        userSteps.projectionDispatch = null
        if (receipt == PhoneControlProjectionReceipt.EXPIRED && next == null && replay != null) {
            if (owner != null && PhoneControlProtectedCheckpointRegistry.ownsFreshProjection(owner)) {
                PhoneControlSetupNotification.show(
                    this@finishAndReplayProjectionReentry,
                    phoneControlString(R.string.phone_control_activation_projection_needed),
                    PhoneControlActivity.resumeCaptureIntent(this@finishAndReplayProjectionReentry),
                )
            } else if (owner == null) {
                showPhoneControlToast(R.string.phone_control_activation_start_failed_toast)
            }
        }
        finish()
        if (next != null && !(shouldSkipProjectionReentry(next))) {
            runCatching { startActivity(next) }.onFailure {
                PhoneControlSetupNotification.show(
                    this@finishAndReplayProjectionReentry,
                    phoneControlString(R.string.phone_control_private_setup_pending),
                    next,
                )
            }
        }
    }
}

private const val PROJECTION_TAG = "SGTPhoneControlActivation"


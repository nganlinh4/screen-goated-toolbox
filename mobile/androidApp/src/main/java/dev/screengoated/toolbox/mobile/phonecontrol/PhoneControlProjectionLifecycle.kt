package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Intent
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionProvider
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionStartResult
import dev.screengoated.toolbox.mobile.phonecontrol.projection.phoneControlProjectionGrant
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeCode
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimePhase
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlActivity

internal fun PhoneControlService.startWithProjection(intent: Intent?) {
    val grant = intent?.phoneControlProjectionGrant()
    if (grant == null) {
        projectionFailure("projection_grant_missing")
        return
    }
    enterForeground()
    when (
        val started = PhoneControlProjectionProvider.start(
            context = this,
            grant = grant,
            onProjectionStopped = {
                mainHandler.post(this::projectionStoppedByPlatform)
            },
        )
    ) {
        is PhoneControlProjectionStartResult.Ready -> {
            projectionActive = true
            PhoneControlSetupNotification.clear(this)
            publish(
                PhoneControlServiceState(
                    running = true,
                    phase = PhoneControlRuntimePhase.STARTING,
                    code = PhoneControlRuntimeCode.STARTING,
                    userMessage = phoneControlString(R.string.phone_control_status_starting),
                ),
            )
            startRuntime()
        }
        is PhoneControlProjectionStartResult.Failure -> projectionFailure(started.code)
    }
}

internal fun PhoneControlService.projectionStoppedByPlatform() {
    if (!projectionActive) return
    projectionActive = false
    Log.w("SGTPhoneControlService", "projection_terminal reason=platform")
    stopReason = "projection_revoked"
    preserveFailureOnDestroy = true
    runtime?.stop()
    runtime = null
    publish(
        PhoneControlServiceState(
            running = false,
            phase = PhoneControlRuntimePhase.ERROR,
            code = PhoneControlRuntimeCode.SCREEN_SHARE_REQUIRED,
            userMessage = phoneControlString(R.string.phone_control_status_projection_required),
        ),
    )
    requestServiceTeardown()
}

internal fun PhoneControlService.projectionFailure(code: String) {
    Log.w("SGTPhoneControlService", "projection_start_failed code=$code")
    stopReason = code
    preserveFailureOnDestroy = true
    publish(
        PhoneControlServiceState(
            running = false,
            phase = PhoneControlRuntimePhase.ERROR,
            code = PhoneControlRuntimeCode.SCREEN_SHARE_REQUIRED,
            userMessage = phoneControlString(R.string.phone_control_status_projection_required),
        ),
    )
    requestServiceTeardown()
}

internal fun PhoneControlService.releaseProjection() {
    projectionActive = false
    PhoneControlProjectionProvider.stop()
}

internal fun PhoneControlService.attachProjection(intent: Intent) {
    val candidate = runtime
    val token = protectedCheckpoint.activeToken
    val grant = intent.phoneControlProjectionGrant()
    val owner = intent.getStringExtra(PhoneControlService.EXTRA_PROJECTION_CHECKPOINT_OWNER)
    if (candidate == null || token == null || grant == null) {
        Log.w(
            "SGTPhoneControlService",
            "projection_attach accepted=false reason=invalid_runtime_or_grant",
        )
        if (candidate == null) requestServiceTeardown()
        return
    }
    val started = startOwnedFreshProtectedProjection(token, owner) {
        PhoneControlProjectionProvider.start(
            context = this,
            grant = grant,
            onProjectionStopped = {
                    mainHandler.post(this::projectionStoppedByPlatform)
            },
        )
    } ?: run {
        Log.w("SGTPhoneControlService", "projection_attach accepted=false reason=stale_checkpoint")
        return
    }
    when (started) {
        is PhoneControlProjectionStartResult.Ready -> {
            projectionActive = true
            val accepted = acceptFreshProtectedProjection(
                token = token,
                attach = { protectedCheckpoint.attachFresh(it, candidate) },
                release = this::releaseProjection,
                accepted = {
                    protectedSetup.onProjectionAttached(authoritySetup) {
                        resumeSelectedAuthoritySetup()
                    }
                },
            )
            if (!accepted) {
                Log.e("SGTPhoneControlService", "projection_attach accepted=false reason=checkpoint_owner_lost")
                showCaptureRetryIfOwned(token, owner)
                return
            }
            Log.i(
                "SGTPhoneControlService",
                "projection_attach accepted=true runtime_reused=true " +
                    "visual_evidence=true",
            )
        }
        is PhoneControlProjectionStartResult.Failure -> {
            Log.w("SGTPhoneControlService", "projection_attach accepted=false code=${started.code}")
            showCaptureRetryIfOwned(token, owner)
        }
    }
}

internal fun PhoneControlService.showCaptureRetryIfOwned(
    token: PhoneControlProtectedCheckpointToken,
    owner: String?,
) {
    if (protectedCheckpoint.activeToken != token ||
        !PhoneControlProtectedCheckpointRegistry.ownsFreshProjection(token, owner)
    ) return
    PhoneControlSetupNotification.show(
        this,
        phoneControlString(R.string.phone_control_private_setup_pending),
        PhoneControlActivity.resumeCaptureIntent(this),
    )
}


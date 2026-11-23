package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.content.Intent
import androidx.lifecycle.ViewModel
import androidx.activity.result.ActivityResult
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PlatformUserStepSlot
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlProjectionDispatch
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlProjectionDispatches
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionGrant
import java.util.UUID

internal sealed interface PhoneControlProjectionRequest {
    val sequence: Long
    data class Activation(
        val generation: String,
        override val sequence: Long = PhoneControlProjectionDispatches.currentIntentSequence(),
    ) : PhoneControlProjectionRequest
    data class Protected(
        val owner: String,
        val generation: String,
        override val sequence: Long = PhoneControlProjectionDispatches.currentIntentSequence(),
    ) : PhoneControlProjectionRequest
}

internal enum class ProjectionReentryKind { ACTIVATE, RESUME, CANCEL, OTHER }
internal enum class ProjectionReentryDisposition { PROCEED, CANCEL, COALESCE, DEFER }

internal fun retireUnownedRestoredProjection(
    savedInFlight: Boolean,
    hasLiveOwner: Boolean,
): Boolean = savedInFlight && !hasLiveOwner

internal fun projectionReentryDisposition(
    request: PhoneControlProjectionRequest?,
    settling: Boolean,
    incoming: ProjectionReentryKind,
    ownerStillCurrent: Boolean,
): ProjectionReentryDisposition = when {
    request == null && !settling -> ProjectionReentryDisposition.PROCEED
    incoming == ProjectionReentryKind.CANCEL -> ProjectionReentryDisposition.CANCEL
    request is PhoneControlProjectionRequest.Activation && incoming == ProjectionReentryKind.ACTIVATE &&
        ownerStillCurrent ->
        ProjectionReentryDisposition.COALESCE
    request is PhoneControlProjectionRequest.Protected && incoming == ProjectionReentryKind.RESUME &&
        ownerStillCurrent -> ProjectionReentryDisposition.COALESCE
    else -> ProjectionReentryDisposition.DEFER
}

internal class PhoneControlUserStepState : ViewModel() {
    val permission = PlatformUserStepSlot()
    val settings = PlatformUserStepSlot()
    val projection = PlatformUserStepSlot()
    val projectionLauncherKey: String = "phone-control-projection-${UUID.randomUUID()}"
    var projectionRequest: PhoneControlProjectionRequest? = null
    var pendingProjectionResult: ActivityResult? = null
    var projectionSettling = false
    var settlingProjectionOwner: String? = null
    var projectionDispatch: PhoneControlProjectionDispatch? = null
    var projectionGrant: PhoneControlProjectionGrant? = null
    var projectionGrantGeneration: String? = null
    var deferredReentrySequence: Long? = null
        private set
    var deferredReentry: Intent? = null
        set(value) {
            field = value
            deferredReentrySequence = value?.let {
                PhoneControlProjectionDispatches.currentIntentSequence()
            }
        }

    val hasProjectionOwnership: Boolean
        get() = projectionRequest != null || projectionSettling || projectionGrant != null

    fun adoptProjectionDispatch(dispatch: PhoneControlProjectionDispatch): Boolean {
        if (hasProjectionOwnership) return false
        projectionDispatch = dispatch
        projectionSettling = true
        return true
    }
    val shizuku = PlatformUserStepSlot()
    val root = PlatformUserStepSlot()

    fun releaseProjectionConsent() {
        when (val request = projectionRequest) {
            is PhoneControlProjectionRequest.Activation ->
                PhoneControlProjectionDispatches.releaseConsent(request.generation)
            is PhoneControlProjectionRequest.Protected ->
                PhoneControlProjectionDispatches.releaseConsent(request.generation)
            null -> Unit
        }
        projectionGrantGeneration?.let(PhoneControlProjectionDispatches::releaseConsent)
    }

    override fun onCleared() {
        releaseProjectionConsent()
        permission.finish()
        settings.finish()
        projection.finish()
        projectionRequest = null
        pendingProjectionResult = null
        projectionSettling = false
        settlingProjectionOwner = null
        projectionDispatch = null
        projectionGrant = null
        projectionGrantGeneration = null
        deferredReentry = null
        shizuku.finish()
        root.finish()
    }
}

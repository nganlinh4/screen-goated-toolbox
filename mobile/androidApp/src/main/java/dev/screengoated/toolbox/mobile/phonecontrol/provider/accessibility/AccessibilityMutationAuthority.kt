package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds

internal data class AccessibilitySurfaceLease(
    val observationGeneration: Long,
    val displayId: Int,
    val windowId: Long,
    val packageOrSurface: String,
    val windowLayer: Int,
    val bounds: TargetBounds,
    val authority: AccessibilityTargetAuthority,
    val controllerOwned: Boolean,
) {
    init {
        require(observationGeneration > 0)
        require(displayId >= 0)
        require(windowId >= 0)
        require(packageOrSurface.isNotBlank())
    }
}

internal enum class AccessibilityMutationKind(val canCommitEffect: Boolean) {
    SEMANTIC_READ(false),
    SEMANTIC_WRITE(false),
    SEMANTIC_COMMIT(true),
    POINTER_ACTIVATE(true),
    LONG_PRESS(true),
    NAVIGATION_GESTURE(false),
    TEXT_EDIT(false),
    TEXT_SUBMIT(true),
    KEY_SEQUENCE(true),
    COMMAND_EXECUTION(true),
}

internal data class AccessibilityCommandDispatchLease(
    val observationGeneration: Long,
) {
    init {
        require(observationGeneration > 0)
    }
}

internal fun AccessibilityObservation.surfaceLease(
    displayId: Int,
    windowId: Long,
): AccessibilitySurfaceLease? {
    val window = windows.singleOrNull { candidate ->
        candidate.displayId == displayId && candidate.id.toLong() == windowId
    } ?: return null
    return window.surfaceLease(generation)
}

internal fun AccessibilityWindowSnapshot.surfaceLease(
    observationGeneration: Long,
): AccessibilitySurfaceLease? {
    val packageIdentity = packageName?.takeIf(String::isNotBlank) ?: return null
    return AccessibilitySurfaceLease(
        observationGeneration = observationGeneration,
        displayId = displayId,
        windowId = id.toLong(),
        packageOrSurface = packageIdentity,
        windowLayer = layer,
        bounds = bounds,
        authority = targetAuthority,
        controllerOwned = controllerOwned,
    )
}

internal fun accessibilitySurfaceName(window: AccessibilityWindowSnapshot): String =
    window.packageName?.takeIf(String::isNotBlank)
        ?: window.title?.takeIf(String::isNotBlank)
        ?: "android-window-${window.id}"

internal fun authorityFailure(
    authority: AccessibilityTargetAuthority,
    kind: AccessibilityMutationKind,
    confirmed: Boolean,
): AccessibilityProviderResult.Failure? = when {
    authority == AccessibilityTargetAuthority.OS_OWNED_USER_STEP ->
        AccessibilityProviderResult.Failure(
            code = "os_owned_confirmation",
            message = "This Android-owned step must be completed by the user.",
            retryable = true,
            requiredUserStep = "complete_os_owned_confirmation",
        )
    authority == AccessibilityTargetAuthority.CONSEQUENTIAL &&
        kind.canCommitEffect && !confirmed ->
        AccessibilityProviderResult.Failure(
            code = "confirmation_required",
            message = "This structurally consequential action requires explicit confirmation.",
            retryable = true,
            requiredUserStep = "confirm_consequential_action",
        )
    else -> null
}

internal fun AccessibilityActionVerb.mutationKind(): AccessibilityMutationKind = when (this) {
    AccessibilityActionVerb.FILL -> AccessibilityMutationKind.SEMANTIC_WRITE
    AccessibilityActionVerb.SELECT -> AccessibilityMutationKind.SEMANTIC_READ
    AccessibilityActionVerb.CLICK,
    AccessibilityActionVerb.ACTIVATE,
    AccessibilityActionVerb.SUBMIT,
    AccessibilityActionVerb.TOGGLE,
    -> AccessibilityMutationKind.SEMANTIC_COMMIT
}

internal fun AccessibilityObservation.commandDispatchAuthorityFailure():
    AccessibilityProviderResult.Failure? {
    val activeWindows = windows.filter { window -> window.active || window.focused }
    if (activeWindows.any { window ->
            window.targetAuthority == AccessibilityTargetAuthority.OS_OWNED_USER_STEP
        }
    ) {
        return authorityFailure(
            AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
            AccessibilityMutationKind.COMMAND_EXECUTION,
            confirmed = false,
        )
    }
    val interactiveWindows = windows.filter { window ->
        !window.controllerOwned && (window.active || window.focused)
    }
    return if (interactiveWindows.isEmpty() || interactiveWindows.any { it.packageName.isNullOrBlank() }) {
        unknownSurfaceAuthority("The active surface has no platform authority identity.")
    } else {
        null
    }
}

internal fun validateSurfaceMutationLease(
    observation: AccessibilityObservation?,
    currentGeneration: Long,
    lease: AccessibilitySurfaceLease,
    kind: AccessibilityMutationKind,
    confirmed: Boolean,
    affectedBounds: TargetBounds?,
): AccessibilityProviderResult.Failure? {
    if (observation == null ||
        observation.generation != lease.observationGeneration ||
        currentGeneration != lease.observationGeneration
    ) {
        return staleSurfaceMutation("The surface does not belong to the current observation.")
    }
    val window = observation.windows.singleOrNull { candidate ->
        candidate.displayId == lease.displayId && candidate.id.toLong() == lease.windowId
    } ?: return staleSurfaceMutation("The surface no longer exists.")
    if (window.packageName.isNullOrBlank()) {
        return AccessibilityProviderResult.Failure(
            code = "surface_authority_unknown",
            message = "The surface has no platform package identity for mutation authority.",
            retryable = true,
            freshObservationRequired = true,
        )
    }
    if (window.surfaceLease(observation.generation) != lease) {
        return staleSurfaceMutation("The surface identity changed before input dispatch.")
    }
    if (lease.controllerOwned) {
        return AccessibilityProviderResult.Failure(
            code = "controller_surface_blocked",
            message = "Phone Control cannot target its own Accessibility overlay.",
            retryable = false,
        )
    }
    affectedBounds?.let { bounds ->
        if (!lease.bounds.contains(bounds)) {
            return staleSurfaceMutation("The input geometry is outside its leased surface.")
        }
    }
    val higherWindows = observation.windows.filter { candidate ->
        candidate.displayId == lease.displayId &&
            !(candidate.controllerOwned && !isApplicationContentWindowType(candidate.type)) &&
            candidate.layer > lease.windowLayer
    }
    if (higherWindows.any { candidate ->
            (candidate.active || candidate.focused) &&
                candidate.targetAuthority == AccessibilityTargetAuthority.OS_OWNED_USER_STEP
        }
    ) {
        return authorityFailure(
            AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
            kind,
            confirmed,
        )
    }
    if (higherWindows.any { candidate ->
            (candidate.active || candidate.focused) && candidate.packageName.isNullOrBlank()
        }
    ) {
        return unknownSurfaceAuthority("A higher active surface has unknown platform authority.")
    }
    affectedBounds?.let { bounds ->
        if (higherWindows.any { candidate -> candidate.bounds.intersects(bounds) }) {
            return staleSurfaceMutation("A higher surface can intercept this input geometry.")
        }
    }
    return authorityFailure(lease.authority, kind, confirmed)
}

private fun staleSurfaceMutation(message: String) = AccessibilityProviderResult.Failure(
    code = "stale_target",
    message = message,
    retryable = true,
    freshObservationRequired = true,
)

private fun unknownSurfaceAuthority(message: String) = AccessibilityProviderResult.Failure(
    code = "surface_authority_unknown",
    message = message,
    retryable = true,
    freshObservationRequired = true,
)

private fun TargetBounds.contains(other: TargetBounds): Boolean =
    other.left >= left && other.top >= top && other.right <= right && other.bottom <= bottom

private fun TargetBounds.intersects(other: TargetBounds): Boolean =
    left < other.right && right > other.left && top < other.bottom && bottom > other.top

package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.view.accessibility.AccessibilityNodeInfo
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

internal data class AccessibilityReversibleSelfTestOutcome(
    val generation: Long,
    val accessibilityFocusVerified: Boolean,
    val stateRestored: Boolean,
)

/**
 * Exercises only Accessibility focus and its inverse on SGT's current Activity.
 * This seam is not registered as a Phone Control tool and itself permits only
 * the reversible focus transition described here.
 */
internal suspend fun performReversibleLocalControlSelfTest(
    provider: PhoneControlAccessibilityProvider,
    expectedGeneration: Long,
): AccessibilityProviderResult<AccessibilityReversibleSelfTestOutcome> =
    withContext(NonCancellable) {
        val capture = provider.currentCaptureForLocalSelfTest(expectedGeneration)
            ?: return@withContext staleSelfTest()
        val servicePackage = provider.currentServicePackage ?: return@withContext staleSelfTest()
        val candidates = reversibleSelfTestCandidates(
            capture,
            expectedGeneration,
            servicePackage,
        )
        if (candidates.isEmpty()) {
            return@withContext missingSelfTestTarget()
        }

        val focusedLease = when (
            val dispatched = provider.onServiceMain(failureEffect = EffectCertainty.UNKNOWN) { service ->
                val currentCapture = provider.currentCaptureForLocalSelfTest(expectedGeneration)
                    ?: return@onServiceMain staleSelfTest()
                for (lease in candidates) {
                    validateReversibleSelfTestLease(
                        currentCapture.observation,
                        provider.observationGeneration,
                        lease,
                        service.packageName,
                    )?.let { continue }
                    val node = resolveAccessibilityNode(service, lease) ?: continue
                    if (!node.matches(lease) || !node.isVisibleToUser || !node.isEnabled ||
                        node.isAccessibilityFocused ||
                        !node.supportsAction(AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS)
                    ) {
                        continue
                    }
                    if (node.performAction(AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS)) {
                        return@onServiceMain AccessibilityProviderResult.Success(lease)
                    }
                }
                missingSelfTestTarget()
            }
        ) {
            is AccessibilityProviderResult.Failure -> return@withContext dispatched
            is AccessibilityProviderResult.Success -> dispatched.value
        }

        delay(SELF_TEST_SETTLE_MS)
        val focusVerified = provider.onServiceMain(failureEffect = EffectCertainty.UNKNOWN) { service ->
            val node = resolveAccessibilityNode(service, focusedLease)
            val stable = node?.matchesIgnoringActions(focusedLease) == true
            val verified = stable && node.isAccessibilityFocused
            if (stable) {
                node.performAction(AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS)
            }
            AccessibilityProviderResult.Success(verified)
        }.successValueOrFalse()

        delay(SELF_TEST_SETTLE_MS)
        val restored = provider.onServiceMain { service ->
            val node = resolveAccessibilityNode(service, focusedLease)
            AccessibilityProviderResult.Success(
                node?.matchesIgnoringActions(focusedLease) == true &&
                    node.isAccessibilityFocused.not(),
            )
        }.successValueOrFalse()

        when {
            !restored -> AccessibilityProviderResult.Failure(
                code = "control_state_restore_failed",
                message = "The reversible Accessibility focus check did not restore local UI state.",
                retryable = true,
                freshObservationRequired = true,
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
            )
            !focusVerified -> AccessibilityProviderResult.Failure(
                code = "control_effect_unverified",
                message = "Accessibility did not verify the reversible local focus transition.",
                retryable = true,
            )
            else -> AccessibilityProviderResult.Success(
                AccessibilityReversibleSelfTestOutcome(
                    generation = expectedGeneration,
                    accessibilityFocusVerified = true,
                    stateRestored = true,
                ),
            )
        }
    }

internal fun reversibleSelfTestCandidates(
    capture: AccessibilityCapture,
    currentGeneration: Long,
    servicePackage: String,
): List<AccessibilityTargetLease> = capture.leases.values
    .asSequence()
    .filter { lease ->
        AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS in lease.fingerprint.actions &&
            validateReversibleSelfTestLease(
                capture.observation,
                currentGeneration,
                lease,
                servicePackage,
            ) == null
    }
    .sortedBy(AccessibilityTargetLease::id)
    .toList()

internal fun validateReversibleSelfTestLease(
    observation: AccessibilityObservation?,
    currentGeneration: Long,
    lease: AccessibilityTargetLease,
    servicePackage: String,
): AccessibilityProviderResult.Failure? {
    if (observation == null || observation.generation != lease.identity.snapshotGeneration ||
        currentGeneration != lease.identity.snapshotGeneration
    ) {
        return staleSelfTest()
    }
    val window = observation.windows.singleOrNull { candidate ->
        candidate.displayId == lease.identity.displayId &&
            candidate.id.toLong() == lease.identity.windowId
    } ?: return staleSelfTest()
    if (window.surfaceLease(observation.generation) != lease.surfaceLease ||
        lease.identity.packageOrSurface != lease.surfaceLease.packageOrSurface ||
        lease.fingerprint.packageName != lease.surfaceLease.packageOrSurface ||
        !lease.surfaceLease.bounds.containsSelfTestBounds(lease.identity.bounds)
    ) {
        return staleSelfTest()
    }
    if (window.controllerOwned || lease.surfaceLease.controllerOwned ||
        !isApplicationContentWindowType(window.type) ||
        window.packageName != servicePackage ||
        lease.surfaceLease.packageOrSurface != servicePackage ||
        !window.active || !window.focused
    ) {
        return missingSelfTestTarget()
    }
    authorityFailure(
        strongestAccessibilityAuthority(lease.surfaceLease.authority, lease.authority),
        AccessibilityMutationKind.SEMANTIC_READ,
        confirmed = false,
    )?.let { return it }
    if (lease.authority != AccessibilityTargetAuthority.ROUTINE ||
        lease.surfaceLease.authority != AccessibilityTargetAuthority.ROUTINE
    ) {
        return missingSelfTestTarget()
    }
    val higherWindows = observation.windows.filter { candidate ->
            candidate.displayId == window.displayId &&
            candidate.layer > window.layer &&
            !(candidate.controllerOwned && !isApplicationContentWindowType(candidate.type))
    }
    if (higherWindows.any { candidate ->
            (candidate.active || candidate.focused) &&
                candidate.targetAuthority == AccessibilityTargetAuthority.OS_OWNED_USER_STEP
        }
    ) {
        return authorityFailure(
            AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
            AccessibilityMutationKind.SEMANTIC_READ,
            confirmed = false,
        )
    }
    if (higherWindows.any { candidate ->
            (candidate.active || candidate.focused) && candidate.packageName.isNullOrBlank()
        }
    ) {
        return AccessibilityProviderResult.Failure(
            code = "surface_authority_unknown",
            message = "A higher active surface has no platform authority identity.",
            retryable = true,
            freshObservationRequired = true,
        )
    }
    if (higherWindows.any { candidate -> candidate.bounds.intersectsSelfTestBounds(lease.identity.bounds) }) {
        return staleSelfTest()
    }
    return null
}

private fun AccessibilityProviderResult<Boolean>.successValueOrFalse(): Boolean =
    (this as? AccessibilityProviderResult.Success)?.value == true

private fun staleSelfTest() = AccessibilityProviderResult.Failure(
    code = "stale_target",
    message = "The local self-test target does not belong to the current observation.",
    retryable = true,
    freshObservationRequired = true,
)

private fun missingSelfTestTarget() = AccessibilityProviderResult.Failure(
    code = "control_target_missing",
    message = "No eligible reversible local Accessibility focus target is available.",
    retryable = true,
)

private fun TargetBounds.containsSelfTestBounds(other: TargetBounds): Boolean =
    other.left >= left && other.top >= top && other.right <= right && other.bottom <= bottom

private fun TargetBounds.intersectsSelfTestBounds(other: TargetBounds): Boolean =
    left < other.right && right > other.left && top < other.bottom && bottom > other.top

private const val SELF_TEST_SETTLE_MS = 80L

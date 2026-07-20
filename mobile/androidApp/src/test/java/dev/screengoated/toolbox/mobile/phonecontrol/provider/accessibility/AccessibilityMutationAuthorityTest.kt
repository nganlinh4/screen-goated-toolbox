package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AccessibilityMutationAuthorityTest {
    @Test
    fun authorityMatrixUsesOnlyStructuralAuthorityAndOperationKind() {
        AccessibilityMutationKind.entries.forEach { kind ->
            assertNull(authorityFailure(AccessibilityTargetAuthority.ROUTINE, kind, false))
            assertEquals(
                "os_owned_confirmation",
                authorityFailure(AccessibilityTargetAuthority.OS_OWNED_USER_STEP, kind, false)?.code,
            )
            assertEquals(
                "os_owned_confirmation",
                authorityFailure(AccessibilityTargetAuthority.OS_OWNED_USER_STEP, kind, true)?.code,
            )
        }

        AccessibilityMutationKind.entries.filter { it.canCommitEffect }.forEach { kind ->
            val blocked = authorityFailure(AccessibilityTargetAuthority.CONSEQUENTIAL, kind, false)
            assertEquals("confirmation_required", blocked?.code)
            assertEquals("confirm_consequential_action", blocked?.requiredUserStep)
            assertNull(authorityFailure(AccessibilityTargetAuthority.CONSEQUENTIAL, kind, true))
        }
        AccessibilityMutationKind.entries.filterNot { it.canCommitEffect }.forEach { kind ->
            assertNull(authorityFailure(AccessibilityTargetAuthority.CONSEQUENTIAL, kind, false))
        }
    }

    @Test
    fun mutationSurfaceRequiresPlatformPackageIdentity() {
        assertNull(window(packageName = null).surfaceLease(GENERATION))
        assertNull(window(packageName = "").surfaceLease(GENERATION))
        assertEquals("fixture.app", window().surfaceLease(GENERATION)?.packageOrSurface)
    }

    @Test
    fun exactCurrentLeaseAllowsRoutineInput() {
        val observation = observation(listOf(window()))
        val lease = requireNotNull(observation.surfaceLease(DISPLAY, WINDOW_ID))

        assertNull(
            validateSurfaceMutationLease(
                observation,
                GENERATION,
                lease,
                AccessibilityMutationKind.POINTER_ACTIVATE,
                confirmed = false,
                affectedBounds = TargetBounds(10, 10, 11, 11),
            ),
        )
    }

    @Test
    fun staleIdentityAndOutOfLeaseGeometryFailBeforeDispatch() {
        val observation = observation(listOf(window()))
        val lease = requireNotNull(observation.surfaceLease(DISPLAY, WINDOW_ID))

        assertEquals(
            "stale_target",
            validateSurfaceMutationLease(
                observation,
                GENERATION + 1,
                lease,
                AccessibilityMutationKind.POINTER_ACTIVATE,
                false,
                TargetBounds(10, 10, 11, 11),
            )?.code,
        )
        assertEquals(
            "stale_target",
            validateSurfaceMutationLease(
                observation,
                GENERATION,
                lease,
                AccessibilityMutationKind.POINTER_ACTIVATE,
                false,
                TargetBounds(250, 10, 251, 11),
            )?.code,
        )
    }

    @Test
    fun higherWindowCannotBeBypassedWithUnderlyingLease() {
        val underlying = window(layer = 1)
        val intercepting = window(
            id = 8,
            layer = 2,
            packageName = "fixture.overlay",
            bounds = TargetBounds(40, 40, 90, 90),
        )
        val observation = observation(listOf(underlying, intercepting))
        val lease = requireNotNull(observation.surfaceLease(DISPLAY, WINDOW_ID))

        assertEquals(
            "stale_target",
            validateSurfaceMutationLease(
                observation,
                GENERATION,
                lease,
                AccessibilityMutationKind.POINTER_ACTIVATE,
                false,
                TargetBounds(50, 50, 51, 51),
            )?.code,
        )
    }

    @Test
    fun globalNavigationUsesExactLeaseWithoutPointerOcclusion() {
        val underlying = window(layer = 1)
        val inactiveHigherWindow = window(
            id = 8,
            layer = 2,
            packageName = "fixture.overlay",
            bounds = TargetBounds(40, 40, 90, 90),
            active = false,
            focused = false,
        )
        val observation = observation(listOf(underlying, inactiveHigherWindow))
        val lease = requireNotNull(observation.surfaceLease(DISPLAY, WINDOW_ID))

        assertNull(
            validateSurfaceMutationLease(
                observation,
                GENERATION,
                lease,
                AccessibilityMutationKind.NAVIGATION_GESTURE,
                confirmed = false,
                affectedBounds = null,
            ),
        )
    }

    @Test
    fun activeUnknownAuthorityCannotBeBypassedByTargetsOrCommands() {
        val underlying = window(active = false, focused = false)
        val unknown = window(
            id = 8,
            layer = 2,
            packageName = null,
            bounds = TargetBounds(40, 40, 90, 90),
        )
        val captured = observation(listOf(underlying, unknown))
        val lease = requireNotNull(captured.surfaceLease(DISPLAY, WINDOW_ID))

        assertEquals(
            "surface_authority_unknown",
            validateSurfaceMutationLease(
                captured,
                GENERATION,
                lease,
                AccessibilityMutationKind.POINTER_ACTIVATE,
                false,
                TargetBounds(5, 5, 6, 6),
            )?.code,
        )
        assertEquals(
            "surface_authority_unknown",
            captured.commandDispatchAuthorityFailure()?.code,
        )
        assertEquals(
            "surface_authority_unknown",
            observation(emptyList()).commandDispatchAuthorityFailure()?.code,
        )
    }

    @Test
    fun activeOsOwnedWindowBlocksUnderlyingInputEvenOutsideItsBounds() {
        val underlying = window(layer = 1)
        val confirmation = window(
            id = 8,
            layer = 2,
            packageName = "fixture.authority",
            bounds = TargetBounds(40, 40, 90, 90),
            authority = AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
        )
        val observation = observation(listOf(underlying, confirmation))
        val lease = requireNotNull(observation.surfaceLease(DISPLAY, WINDOW_ID))
        val failure = validateSurfaceMutationLease(
            observation,
            GENERATION,
            lease,
            AccessibilityMutationKind.NAVIGATION_GESTURE,
            confirmed = true,
            affectedBounds = TargetBounds(5, 5, 6, 6),
        )

        assertEquals("os_owned_confirmation", failure?.code)
        assertEquals("complete_os_owned_confirmation", failure?.requiredUserStep)
    }

    @Test
    fun elevatedCommandIsBlockedOnlyByAnActiveOsOwnedStep() {
        val inactiveConfirmation = window(
            id = 8,
            layer = 2,
            packageName = "fixture.authority",
            authority = AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
            active = false,
            focused = false,
        )
        assertNull(observation(listOf(window(), inactiveConfirmation)).commandDispatchAuthorityFailure())

        val activeConfirmation = inactiveConfirmation.copy(active = true)
        val failure = observation(listOf(window(active = false), activeConfirmation))
            .commandDispatchAuthorityFailure()
        assertEquals("os_owned_confirmation", failure?.code)
        assertEquals("complete_os_owned_confirmation", failure?.requiredUserStep)
    }

    @Test
    fun activeControllerApplicationCannotHideAPlatformUserStepFromCommandPreflight() {
        val platformStep = window(
            packageName = "fixture.controller",
            authority = AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
            controllerOwned = true,
        )

        val failure = observation(listOf(platformStep)).commandDispatchAuthorityFailure()

        assertEquals("os_owned_confirmation", failure?.code)
        assertEquals(EffectCertainty.PROVEN_NO_EFFECT, failure?.effect)
    }

    @Test
    fun overlayCandidateIsScopedToARealLayeredPlatformSurface() {
        val policy = AccessibilityTargetAuthorityPolicy(
            osOwnedUserStepPackages = emptySet(),
            osOwnedOverlayCandidatePackages = setOf("fixture.platform"),
        )
        val ordinary = captured(packageName = "fixture.platform", layer = 1)
        assertEquals(
            AccessibilityTargetAuthority.ROUTINE,
            policy.classifyWindow(ordinary, listOf(ordinary)),
        )

        val app = captured(packageName = "fixture.app", layer = 1)
        val prompt = captured(packageName = "fixture.platform", layer = 2)
        assertEquals(
            AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
            policy.classifyWindow(prompt, listOf(app, prompt)),
        )
        assertTrue(policy.classifyWindow(app, listOf(app, prompt)) == AccessibilityTargetAuthority.ROUTINE)
    }

    private fun observation(windows: List<AccessibilityWindowSnapshot>) = AccessibilityObservation(
        generation = GENERATION,
        observedAtMs = 1,
        displayRotation = 0,
        densityDpi = 320,
        windows = windows,
        elements = emptyList(),
        truncated = false,
    )

    private fun window(
        id: Int = WINDOW_ID.toInt(),
        layer: Int = 1,
        packageName: String? = "fixture.app",
        bounds: TargetBounds = SURFACE_BOUNDS,
        authority: AccessibilityTargetAuthority = AccessibilityTargetAuthority.ROUTINE,
        active: Boolean = true,
        focused: Boolean = true,
        controllerOwned: Boolean = false,
    ) = AccessibilityWindowSnapshot(
        id = id,
        displayId = DISPLAY,
        layer = layer,
        type = "application",
        title = null,
        packageName = packageName,
        active = active,
        focused = focused,
        bounds = bounds,
        controllerOwned = controllerOwned,
        targetAuthority = authority,
    )

    private fun captured(
        packageName: String,
        layer: Int,
    ) = CapturedAccessibilityWindow(
        displayId = DISPLAY,
        id = layer,
        layer = layer,
        type = "application",
        title = null,
        packageName = packageName,
        active = true,
        focused = true,
        bounds = SURFACE_BOUNDS,
        accessibilityOverlay = false,
        pictureInPicture = false,
        root = Unit,
    )

    private companion object {
        const val GENERATION = 12L
        const val DISPLAY = 0
        const val WINDOW_ID = 4L
        val SURFACE_BOUNDS = TargetBounds(0, 0, 200, 400)
    }
}

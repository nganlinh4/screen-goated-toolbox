package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.view.accessibility.AccessibilityNodeInfo
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlTargetIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AccessibilityReversibleSelfTestTest {
    @Test
    fun selfTestSelectsOnlyCurrentControllerApplicationByStructure() {
        val external = window(id = 1, packageName = "fixture.external", controllerOwned = false)
        val controllerOverlay = window(
            id = 2,
            type = "accessibility_overlay",
            controllerOwned = true,
        )
        val controllerApplication = window(id = 3)
        val observation = observation(listOf(external, controllerOverlay, controllerApplication))
        val leases = listOf(
            lease(external, id = 1),
            lease(controllerOverlay, id = 2),
            lease(controllerApplication, id = 3),
        ).associateBy(AccessibilityTargetLease::id)

        val candidates = reversibleSelfTestCandidates(
            AccessibilityCapture(observation, leases),
            GENERATION,
            CONTROLLER_PACKAGE,
        )

        assertEquals(listOf(3), candidates.map(AccessibilityTargetLease::id))
        assertNull(
            validateReversibleSelfTestLease(
                observation,
                GENERATION,
                candidates.single(),
                CONTROLLER_PACKAGE,
            ),
        )
    }

    @Test
    fun selfTestRejectsStaleMissingActionAndPlatformAuthority() {
        val routine = window(id = 1)
        val observation = observation(listOf(routine))
        val eligible = lease(routine, id = 1)

        assertEquals(
            "stale_target",
            validateReversibleSelfTestLease(
                observation,
                GENERATION + 1,
                eligible,
                CONTROLLER_PACKAGE,
            )?.code,
        )
        val withoutFocus = eligible.copy(
            fingerprint = eligible.fingerprint.copy(actions = setOf(AccessibilityNodeInfo.ACTION_CLICK)),
        )
        assertTrue(
            reversibleSelfTestCandidates(
                AccessibilityCapture(observation, mapOf(1 to withoutFocus)),
                GENERATION,
                CONTROLLER_PACKAGE,
            ).isEmpty(),
        )

        val platformWindow = routine.copy(
            targetAuthority = AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
        )
        val platformLease = lease(
            platformWindow,
            id = 1,
            authority = AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
        )
        assertEquals(
            "os_owned_confirmation",
            validateReversibleSelfTestLease(
                observation(listOf(platformWindow)),
                GENERATION,
                platformLease,
                CONTROLLER_PACKAGE,
            )?.code,
        )
    }

    @Test
    fun higherOsStepBlocksSelfTestWhileOnlyTheOverlayRemainsControllerOwned() {
        val controller = window(id = 1, layer = 1)
        val platformStep = window(
            id = 2,
            layer = 2,
            packageName = "fixture.platform",
            controllerOwned = false,
            authority = AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
        )
        val observation = observation(listOf(controller, platformStep))
        val lease = lease(controller, id = 1)

        assertEquals(
            "os_owned_confirmation",
            validateReversibleSelfTestLease(
                observation,
                GENERATION,
                lease,
                CONTROLLER_PACKAGE,
            )?.code,
        )
        assertNull(
            validateSurfaceMutationLease(
                observation(listOf(controller)),
                GENERATION,
                requireNotNull(controller.surfaceLease(GENERATION)),
                AccessibilityMutationKind.SEMANTIC_READ,
                confirmed = false,
                affectedBounds = TARGET_BOUNDS,
            ),
        )
        val overlay = window(
            id = 3,
            type = "accessibility_overlay",
            controllerOwned = true,
        )
        assertEquals(
            "controller_surface_blocked",
            validateSurfaceMutationLease(
                observation(listOf(overlay)),
                GENERATION,
                requireNotNull(overlay.surfaceLease(GENERATION)),
                AccessibilityMutationKind.SEMANTIC_READ,
                confirmed = false,
                affectedBounds = TARGET_BOUNDS,
            )?.code,
        )
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
        id: Int,
        layer: Int = 1,
        type: String = "application",
        packageName: String = CONTROLLER_PACKAGE,
        controllerOwned: Boolean = false,
        authority: AccessibilityTargetAuthority = AccessibilityTargetAuthority.ROUTINE,
    ) = AccessibilityWindowSnapshot(
        id = id,
        displayId = DISPLAY_ID,
        layer = layer,
        type = type,
        title = null,
        packageName = packageName,
        active = true,
        focused = true,
        bounds = SURFACE_BOUNDS,
        controllerOwned = controllerOwned,
        targetAuthority = authority,
    )

    private fun lease(
        window: AccessibilityWindowSnapshot,
        id: Int,
        authority: AccessibilityTargetAuthority = window.targetAuthority,
    ): AccessibilityTargetLease {
        val surfaceLease = requireNotNull(window.surfaceLease(GENERATION))
        return AccessibilityTargetLease(
            id = id,
            identity = PhoneControlTargetIdentity(
                snapshotGeneration = GENERATION,
                displayId = window.displayId,
                windowId = window.id.toLong(),
                packageOrSurface = requireNotNull(window.packageName),
                nodeOrDocumentIdentity = "${window.id}:$id",
                bounds = TARGET_BOUNDS,
                observationTimestampMs = 1,
            ),
            surfaceLease = surfaceLease,
            childPath = listOf(id),
            fingerprint = AccessibilityNodeFingerprint(
                packageName = requireNotNull(window.packageName),
                className = "android.widget.Button",
                viewId = null,
                bounds = TARGET_BOUNDS,
                actions = setOf(AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS),
                semanticContentHash = 0,
                isProtected = false,
            ),
            accessibilityOverlay = window.type != "application",
            authority = authority,
        )
    }

    private companion object {
        const val GENERATION = 7L
        const val DISPLAY_ID = 0
        const val CONTROLLER_PACKAGE = "fixture.controller"
        val SURFACE_BOUNDS = TargetBounds(0, 0, 200, 200)
        val TARGET_BOUNDS = TargetBounds(10, 10, 40, 40)
    }
}

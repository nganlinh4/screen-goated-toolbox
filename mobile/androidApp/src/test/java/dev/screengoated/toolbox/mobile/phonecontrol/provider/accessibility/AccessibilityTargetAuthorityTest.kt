package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import org.junit.Assert.assertEquals
import org.junit.Test

class AccessibilityTargetAuthorityTest {
    @Test
    fun policyUsesOnlyThePlatformDerivedPackageSet() {
        val policy = AccessibilityTargetAuthorityPolicy(
            osOwnedUserStepPackages = setOf("platform.confirmation"),
        )

        assertEquals(
            AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
            policy.classify("platform.confirmation"),
        )
        assertEquals(
            AccessibilityTargetAuthority.ROUTINE,
            policy.classify("ordinary.application"),
        )
        assertEquals(AccessibilityTargetAuthority.ROUTINE, policy.classify(""))
    }

    @Test
    fun onlyCapabilityDerivedAuthorityNeedsARealOverlayingApplicationWindow() {
        val policy = AccessibilityTargetAuthorityPolicy(
            osOwnedUserStepPackages = emptySet(),
            osOwnedOverlayCandidatePackages = setOf("platform.permission-controller"),
        )
        val app = captured(packageName = "fixture.app", layer = 1)
        val systemSurface = captured(packageName = "platform.permission-controller", layer = 2)

        assertEquals(
            AccessibilityTargetAuthority.ROUTINE,
            policy.classifyWindow(systemSurface, listOf(systemSurface)),
        )
        assertEquals(
            AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
            policy.classifyWindow(systemSurface, listOf(app, systemSurface)),
        )
    }

    @Test
    fun genericPreinstalledApplicationIsNeverAuthorityByInstallationClass() {
        val policy = AccessibilityTargetAuthorityPolicy(
            osOwnedUserStepPackages = emptySet(),
            osOwnedOverlayCandidatePackages = setOf("platform.permission-controller"),
        )
        val app = captured(packageName = "fixture.app", layer = 1)
        val ordinarySystemApp = captured(packageName = "fixture.preinstalled-app", layer = 2)

        assertEquals(
            AccessibilityTargetAuthority.ROUTINE,
            policy.classifyWindow(ordinarySystemApp, listOf(app, ordinarySystemApp)),
        )
    }

    @Test
    fun pendingPlatformSessionOwnsTheActiveApplicationSurfaceStructurally() {
        val policy = AccessibilityTargetAuthorityPolicy(
            osOwnedUserStepPackages = emptySet(),
            platformUserStepActive = true,
        )
        listOf("application", "system").forEach { type ->
            val activeSurface = captured(
                packageName = "fixture.application",
                layer = 1,
                type = type,
            )
            assertEquals(
                AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
                policy.classifyWindow(activeSurface, listOf(activeSurface)),
            )
        }
    }

    @Test
    fun platformDismissActionIsConsequentialWithoutReadingLabels() {
        assertEquals(
            AccessibilityTargetAuthority.ROUTINE,
            structuralNodeAuthority(supportsPlatformDismiss = false),
        )
        assertEquals(
            AccessibilityTargetAuthority.CONSEQUENTIAL,
            structuralNodeAuthority(supportsPlatformDismiss = true),
        )
    }

    private fun captured(
        packageName: String,
        layer: Int,
        type: String = "application",
    ): CapturedAccessibilityWindow<Unit> = CapturedAccessibilityWindow(
        displayId = 0,
        id = layer,
        layer = layer,
        type = type,
        title = null,
        packageName = packageName,
        active = true,
        focused = true,
        bounds = TargetBounds(0, 0, 200, 400),
        accessibilityOverlay = false,
        pictureInPicture = false,
        root = Unit,
    )
}

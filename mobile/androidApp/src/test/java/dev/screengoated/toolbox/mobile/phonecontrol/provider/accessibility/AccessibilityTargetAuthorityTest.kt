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
    fun pendingPlatformSessionOwnsItsResolvedHandlerOnlyForTheTokenLifetime() {
        val policy = AccessibilityTargetAuthorityPolicy(
            osOwnedUserStepPackages = emptySet(),
            platformUserStepActive = true,
            expectedUserStepPackages = setOf("fixture.application"),
        )
        val setupSurface = captured(
            packageName = "fixture.application",
            layer = 1,
        )
        assertEquals(
            AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
            policy.classifyWindow(setupSurface, listOf(setupSurface)),
        )
        val unrelated = captured(packageName = "fixture.other", layer = 1)
        assertEquals(
            AccessibilityTargetAuthority.ROUTINE,
            policy.classifyWindow(unrelated, listOf(unrelated)),
        )
        listOf("application", "system").forEach { type ->
            val modal = captured(
                packageName = "fixture.application",
                layer = 2,
                type = type,
            )
            assertEquals(
                AccessibilityTargetAuthority.OS_OWNED_USER_STEP,
                policy.classifyWindow(modal, listOf(setupSurface, modal)),
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

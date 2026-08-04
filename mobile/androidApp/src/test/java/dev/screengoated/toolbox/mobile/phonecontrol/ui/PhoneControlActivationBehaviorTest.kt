package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.content.Intent
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlAuthorityAutomationDisposition
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlAuthorityAutomationOwner
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlAuthorityResumeDisposition
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlProtectedNavigationDecision
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCapturePolicy
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.phoneControlAuthorityAutomationDisposition
import dev.screengoated.toolbox.mobile.phonecontrol.phoneControlAuthorityResumeDisposition
import dev.screengoated.toolbox.mobile.phonecontrol.phoneControlProtectedNavigationDecision
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuBridgeCondition
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuProtectedSetupAdapter
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbBridgeCondition
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbBridgeProbe
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbProtectedSetupAdapter
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlActivationBehaviorTest {
    @Test
    fun `protected navigation completion requires structural postcondition`() {
        assertEquals(
            PhoneControlProtectedNavigationDecision.WAIT,
            phoneControlProtectedNavigationDecision(
                goalFinished = false,
                postGoalSettled = false,
                attempt = 1,
                maximumAttempts = 4,
                deadlineReached = false,
            ),
        )
        assertEquals(
            PhoneControlProtectedNavigationDecision.WAIT,
            phoneControlProtectedNavigationDecision(
                goalFinished = true,
                postGoalSettled = false,
                attempt = 1,
                maximumAttempts = 4,
                deadlineReached = false,
            ),
        )
        assertEquals(
            PhoneControlProtectedNavigationDecision.RETRY,
            phoneControlProtectedNavigationDecision(
                goalFinished = true,
                postGoalSettled = true,
                attempt = 1,
                maximumAttempts = 4,
                deadlineReached = false,
            ),
        )
        assertEquals(
            PhoneControlProtectedNavigationDecision.EXHAUSTED,
            phoneControlProtectedNavigationDecision(
                goalFinished = true,
                postGoalSettled = true,
                attempt = 4,
                maximumAttempts = 4,
                deadlineReached = false,
            ),
        )
        assertEquals(
            PhoneControlProtectedNavigationDecision.EXHAUSTED,
            phoneControlProtectedNavigationDecision(
                goalFinished = false,
                postGoalSettled = false,
                attempt = 1,
                maximumAttempts = 4,
                deadlineReached = true,
            ),
        )
    }

    @Test
    fun `authority setup automation is single flight per selected provider`() {
        assertEquals(
            PhoneControlAuthorityAutomationDisposition.NONE,
            phoneControlAuthorityAutomationDisposition(
                automationRequested = false,
                requestedProvider = "provider-a",
                activeGoalId = null,
                activeProvider = null,
            ),
        )
        assertEquals(
            PhoneControlAuthorityAutomationDisposition.SUBMIT,
            phoneControlAuthorityAutomationDisposition(
                automationRequested = true,
                requestedProvider = "provider-a",
                activeGoalId = null,
                activeProvider = null,
            ),
        )
        assertEquals(
            PhoneControlAuthorityAutomationDisposition.COALESCE,
            phoneControlAuthorityAutomationDisposition(
                automationRequested = true,
                requestedProvider = "provider-a",
                activeGoalId = 41,
                activeProvider = "provider-a",
            ),
        )
        assertEquals(
            PhoneControlAuthorityAutomationDisposition.BLOCKED,
            phoneControlAuthorityAutomationDisposition(
                automationRequested = true,
                requestedProvider = "provider-b",
                activeGoalId = 41,
                activeProvider = "provider-a",
            ),
        )

        val owner = PhoneControlAuthorityAutomationOwner()
        assertEquals(
            PhoneControlAuthorityAutomationDisposition.SUBMIT,
            owner.disposition(automationRequested = true, requestedProvider = "provider-a"),
        )
        owner.begin(goalId = 41, providerId = "provider-a", checkpointMonitoring = false)
        assertEquals(
            PhoneControlAuthorityAutomationDisposition.COALESCE,
            owner.disposition(automationRequested = true, requestedProvider = "provider-a"),
        )
        val coalesced = owner.coalesce("provider-a", checkpointMonitoring = true)
        assertEquals(41L, coalesced?.goalId)
        assertTrue(coalesced?.checkpointMonitoring == true)
        assertEquals(null, owner.complete(99))
        assertEquals(coalesced, owner.complete(41))
        assertEquals(
            PhoneControlAuthorityAutomationDisposition.SUBMIT,
            owner.disposition(automationRequested = true, requestedProvider = "provider-a"),
        )
    }

    @Test
    fun `protected setup capture policy follows provider interaction requirements`() {
        assertEquals(
            PhoneControlProtectedCapturePolicy.RETAIN_PROJECTION,
            SgtAdbProtectedSetupAdapter.capturePolicy,
        )
        assertEquals(
            PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION,
            ShizukuProtectedSetupAdapter.capturePolicy,
        )
        assertEquals(
            SgtAdbProtectedSetupAdapter.navigationContract,
            ShizukuProtectedSetupAdapter.navigationContract,
        )
        assertTrue(SgtAdbProtectedSetupAdapter.navigationContract.platformCapability.isNotBlank())
        assertTrue(SgtAdbProtectedSetupAdapter.navigationContract.destinationState.isNotBlank())
    }

    @Test
    fun `coordinator reentry retires its external surface before projection`() {
        val flags = PhoneControlActivity.COORDINATOR_REENTRY_FLAGS

        assertTrue(flags and Intent.FLAG_ACTIVITY_NEW_TASK != 0)
        assertTrue(flags and Intent.FLAG_ACTIVITY_CLEAR_TOP != 0)
        assertTrue(flags and Intent.FLAG_ACTIVITY_SINGLE_TOP != 0)
    }

    @Test
    fun `coordinator reentry opts into the platform background launch contract`() {
        assertEquals(
            PhoneControlBackgroundLaunchMode.PLATFORM_DEFAULT,
            phoneControlBackgroundLaunchMode(33),
        )
        assertEquals(
            PhoneControlBackgroundLaunchMode.ALLOWED,
            phoneControlBackgroundLaunchMode(34),
        )
        assertEquals(
            PhoneControlBackgroundLaunchMode.ALLOWED,
            phoneControlBackgroundLaunchMode(35),
        )
        assertEquals(
            PhoneControlBackgroundLaunchMode.ALLOW_ALWAYS,
            phoneControlBackgroundLaunchMode(36),
        )
    }

    @Test
    fun `coordinator reentry retires the old external activity result`() {
        assertEquals(
            PhoneControlExternalResultDisposition.RETIRE_FOR_REENTRY,
            phoneControlExternalResultDisposition(
                reentryPending = true,
                externalStepActive = true,
            ),
        )
        assertEquals(
            PhoneControlExternalResultDisposition.IGNORE_RETIRED,
            phoneControlExternalResultDisposition(
                reentryPending = false,
                externalStepActive = false,
            ),
        )
        assertEquals(
            PhoneControlExternalResultDisposition.HANDLE,
            phoneControlExternalResultDisposition(
                reentryPending = false,
                externalStepActive = true,
            ),
        )
    }

    @Test
    fun `power presentation separates persisted selection from recommendation`() {
        PhoneControlPowerChoice.entries.forEach { selected ->
            PhoneControlPowerChoice.entries.forEach { choice ->
                val presentation = phoneControlPowerChoicePresentation(choice, selected)
                assertEquals(choice == selected, presentation.selected)
                assertEquals(
                    choice == PhoneControlPowerChoice.SGT_ADB,
                    presentation.recommended,
                )
            }
        }
        val unselectedRecommendation = phoneControlPowerChoicePresentation(
            PhoneControlPowerChoice.SGT_ADB,
            PhoneControlPowerChoice.STANDARD,
        )
        assertFalse(unselectedRecommendation.selected)
        assertTrue(unselectedRecommendation.recommended)
    }

    @Test
    fun `authority resume reconciles fresh ready evidence without reopening setup`() {
        assertEquals(
            PhoneControlAuthorityResumeDisposition.NONE,
            phoneControlAuthorityResumeDisposition(null, null),
        )
        assertEquals(
            PhoneControlAuthorityResumeDisposition.NONE,
            phoneControlAuthorityResumeDisposition(
                PhoneControlPowerChoice.STANDARD,
                CapabilityState.READY,
            ),
        )
        assertEquals(
            PhoneControlAuthorityResumeDisposition.READY,
            phoneControlAuthorityResumeDisposition(
                PhoneControlPowerChoice.SGT_ADB,
                CapabilityState.READY,
            ),
        )
        CapabilityState.entries
            .filterNot { it == CapabilityState.READY }
            .forEach { state ->
                assertEquals(
                    PhoneControlAuthorityResumeDisposition.RESUME_SETUP,
                    phoneControlAuthorityResumeDisposition(
                        PhoneControlPowerChoice.SGT_ADB,
                        state,
                    ),
                )
            }
    }

    @Test
    fun `accessibility readiness requires configuration and a live binding`() {
        assertEquals(
            PhoneControlAccessibilityState.DISABLED,
            phoneControlAccessibilityState(configured = false, serviceBound = false),
        )
        assertEquals(
            PhoneControlAccessibilityState.DISABLED,
            phoneControlAccessibilityState(configured = false, serviceBound = true),
        )
        assertEquals(
            PhoneControlAccessibilityState.RECONNECTING,
            phoneControlAccessibilityState(configured = true, serviceBound = false),
        )
        assertEquals(
            PhoneControlAccessibilityState.READY,
            phoneControlAccessibilityState(configured = true, serviceBound = true),
        )
    }

    @Test
    fun `accessibility activation opens settings only for freshly disabled service`() {
        assertEquals(
            PhoneControlAccessibilityResolution.OPEN_SETTINGS,
            phoneControlAccessibilityResolution(
                PhoneControlAccessibilityState.DISABLED,
                reconnectWaitExhausted = false,
            ),
        )
        assertEquals(
            PhoneControlAccessibilityResolution.WAIT,
            phoneControlAccessibilityResolution(
                PhoneControlAccessibilityState.RECONNECTING,
                reconnectWaitExhausted = false,
            ),
        )
        assertEquals(
            PhoneControlAccessibilityResolution.STOP,
            phoneControlAccessibilityResolution(
                PhoneControlAccessibilityState.RECONNECTING,
                reconnectWaitExhausted = true,
            ),
        )
        listOf(false, true).forEach { reconnectWaitExhausted ->
            assertEquals(
                PhoneControlAccessibilityResolution.CONTINUE,
                phoneControlAccessibilityResolution(
                    PhoneControlAccessibilityState.READY,
                    reconnectWaitExhausted,
                ),
            )
        }
    }

    @Test
    fun `Shizuku setup advances on state change without repeating one external step`() {
        val missing = PhoneControlShizukuSetupAttempt(
            ShizukuBridgeCondition.PACKAGE_MISSING,
            PhoneControlShizukuSetupAction.OPEN_STORE,
        )
        val installed = PhoneControlShizukuSetupAttempt(
            ShizukuBridgeCondition.SERVICE_STOPPED,
            PhoneControlShizukuSetupAction.OPEN_MANAGER,
        )

        assertEquals(
            PhoneControlShizukuRepeatDisposition.DISPATCH,
            phoneControlShizukuRepeatDisposition(missing, previous = null, stepActive = false),
        )
        assertEquals(
            PhoneControlShizukuRepeatDisposition.WAIT_FOR_EVENT,
            phoneControlShizukuRepeatDisposition(missing, missing, stepActive = true),
        )
        assertEquals(
            PhoneControlShizukuRepeatDisposition.LEAVE_SELECTED_PENDING,
            phoneControlShizukuRepeatDisposition(missing, missing, stepActive = false),
        )
        assertEquals(
            PhoneControlShizukuRepeatDisposition.DISPATCH,
            phoneControlShizukuRepeatDisposition(installed, missing, stepActive = false),
        )
    }

    @Test
    fun `SGT bridge setup waits for fresh state instead of reopening unchanged settings`() {
        val missing = phoneControlSgtAdbSetupAttempt(
            SgtAdbBridgeProbe(
                state = CapabilityState.NEEDS_USER_STEP,
                condition = SgtAdbBridgeCondition.NOT_PAIRED,
            ),
        )
        val connecting = phoneControlSgtAdbSetupAttempt(
            SgtAdbBridgeProbe(
                state = CapabilityState.DEGRADED,
                condition = SgtAdbBridgeCondition.CONNECTING,
            ),
        )

        assertEquals(
            PhoneControlSgtAdbRepeatDisposition.DISPATCH,
            phoneControlSgtAdbRepeatDisposition(missing, previous = null, stepActive = false),
        )
        assertEquals(
            PhoneControlSgtAdbRepeatDisposition.WAIT_FOR_RETURN,
            phoneControlSgtAdbRepeatDisposition(missing, missing, stepActive = true),
        )
        assertEquals(
            PhoneControlSgtAdbRepeatDisposition.LEAVE_SELECTED_PENDING,
            phoneControlSgtAdbRepeatDisposition(missing, missing, stepActive = false),
        )
        assertEquals(
            PhoneControlSgtAdbRepeatDisposition.DISPATCH,
            phoneControlSgtAdbRepeatDisposition(connecting, missing, stepActive = false),
        )
    }
}

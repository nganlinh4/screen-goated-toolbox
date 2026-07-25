package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import org.junit.Assert.assertEquals
import org.junit.Test

class AccessibilityActionPostconditionTest {
    @Test
    fun `required state mutations cannot flatten a missed postcondition into ok`() {
        listOf(
            AccessibilityActionVerb.FILL,
            AccessibilityActionVerb.SELECT,
            AccessibilityActionVerb.TOGGLE,
        ).forEach { verb ->
            assertEquals(
                "postcondition_not_verified",
                accessibilityActionPostconditionCode(verb, verified = false),
            )
        }
    }

    @Test
    fun `dispatch-only actions keep may-have-occurred success semantics`() {
        listOf(
            AccessibilityActionVerb.CLICK,
            AccessibilityActionVerb.ACTIVATE,
            AccessibilityActionVerb.SUBMIT,
        ).forEach { verb ->
            assertEquals(
                "ok",
                accessibilityActionPostconditionCode(verb, verified = false),
            )
        }
    }

    @Test
    fun `verified postcondition reports ok for every action`() {
        AccessibilityActionVerb.entries.forEach { verb ->
            assertEquals(
                "ok",
                accessibilityActionPostconditionCode(verb, verified = true),
            )
        }
    }
}

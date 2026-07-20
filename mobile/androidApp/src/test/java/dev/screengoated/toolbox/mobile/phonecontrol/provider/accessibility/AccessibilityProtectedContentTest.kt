package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlTargetIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AccessibilityProtectedContentTest {
    @Test
    fun passwordTextCannotReachModelFieldsOrFingerprintEvidence() {
        val first = protectedContent(PROTECTED_CANARY)
        val second = protectedContent("different-value-with-different-length")

        assertNull(first.label)
        assertNull(first.value)
        assertNull(first.hint)
        assertNull(first.stateDescription)
        assertTrue(first.isProtected)
        assertEquals(first.semanticFingerprintHash, second.semanticFingerprintHash)
        assertFalse(first.toString().contains(PROTECTED_CANARY))
        val fingerprint = AccessibilityNodeFingerprint(
            packageName = "test.package",
            className = "android.widget.EditText",
            viewId = "test:id/protected_input",
            bounds = BOUNDS,
            actions = emptySet(),
            semanticContentHash = first.semanticFingerprintHash,
            isProtected = true,
        )
        assertFalse(fingerprint.toString().contains(PROTECTED_CANARY))

        val element = element(first)
        assertFalse(observation(element).toString().contains(PROTECTED_CANARY))
        val adversarialElement = element(
            first.copy(
                label = PROTECTED_CANARY,
                value = PROTECTED_CANARY,
                hint = PROTECTED_CANARY,
                stateDescription = PROTECTED_CANARY,
            ),
        )
        val modelText = observation(adversarialElement).toModelText()
        assertTrue(modelText.contains("privacy=protected"))
        assertFalse(modelText.contains(PROTECTED_CANARY))
        assertFalse(modelText.contains("label="))
        assertFalse(modelText.contains("hint="))
        assertFalse(modelText.contains("state="))
        assertFalse(element.toModelLine().contains("value="))
    }

    private fun protectedContent(text: String) = accessibilityNodeContent(
        isPassword = true,
        contentDescription = SAFE_DESCRIPTION,
        text = text,
        hint = SAFE_HINT,
        stateDescription = SAFE_STATE,
        editable = true,
    )

    private fun element(content: AccessibilityNodeContent): AccessibilityElement = AccessibilityElement(
        id = 1,
        role = "text_field",
        label = content.label,
        value = content.value,
        hint = content.hint,
        stateDescription = content.stateDescription,
        viewId = "test:id/protected_input",
        packageName = "test.package",
        className = "android.widget.EditText",
        bounds = BOUNDS,
        actions = setOf("focus", "fill"),
        enabled = true,
        visible = true,
        focused = false,
        selected = false,
        checked = null,
        isProtected = content.isProtected,
        controllerOwned = false,
        target = TARGET,
    )

    private fun observation(element: AccessibilityElement) = AccessibilityObservation(
        generation = 1,
        observedAtMs = 2,
        displayRotation = 0,
        densityDpi = 420,
        windows = emptyList(),
        elements = listOf(element),
        truncated = false,
    )

    private companion object {
        const val PROTECTED_CANARY = "canary-protected-value-7b9d"
        const val SAFE_DESCRIPTION = "Credential input"
        const val SAFE_HINT = "Enter credential"
        const val SAFE_STATE = "Not empty"
        val BOUNDS = TargetBounds(10, 20, 200, 80)
        val TARGET = PhoneControlTargetIdentity(
            snapshotGeneration = 1,
            displayId = 0,
            windowId = 3,
            packageOrSurface = "test.package",
            nodeOrDocumentIdentity = "3:0:test:id/protected_input",
            bounds = BOUNDS,
            observationTimestampMs = 2,
        )
    }
}

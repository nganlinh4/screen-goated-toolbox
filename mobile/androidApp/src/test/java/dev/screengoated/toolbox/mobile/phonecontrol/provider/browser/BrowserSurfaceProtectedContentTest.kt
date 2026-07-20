package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import dev.screengoated.toolbox.mobile.phonecontrol.provider.sha256
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityElement
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlTargetIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class BrowserSurfaceProtectedContentTest {
    @Test
    fun protectedValueCannotReachCaptureArtifactPreviewOrDigest() {
        val first = captureVisibleBrowserText(listOf(protectedElement(PROTECTED_CANARY)))
        val second = captureVisibleBrowserText(listOf(protectedElement("another-secret")))

        assertEquals(1, first.protectedElements)
        assertEquals(0, first.includedValues)
        assertEquals(first.text, second.text)
        assertEquals(first.text.toByteArray().sha256(), second.text.toByteArray().sha256())
        assertFalse(first.text.contains(PROTECTED_CANARY))
        assertFalse(first.text.toByteArray().toString(Charsets.UTF_8).contains(PROTECTED_CANARY))

        listOf(true, false).forEach { includePreview ->
            val presentation = presentation(first, includePreview)
            val serialized = buildJsonObject {
                put("page", presentation.page)
                put("artifact", presentation.artifact)
                put("completion_proof", presentation.completionProof)
            }.toString()
            assertFalse(serialized.contains(PROTECTED_CANARY))
            assertEquals(includePreview, presentation.page.containsKey("text"))
            assertEquals(includePreview, presentation.artifact.containsKey("preview"))
        }
    }

    @Test
    fun protectedTextLikeFieldsCannotBecomeAnObservedUrl() {
        val observation = AccessibilityObservation(
            generation = 4,
            observedAtMs = 10,
            displayRotation = 0,
            densityDpi = 420,
            windows = listOf(
                AccessibilityWindowSnapshot(
                    id = 9,
                    displayId = 0,
                    layer = 1,
                    type = "application",
                    title = "Browser",
                    packageName = "browser.package",
                    active = true,
                    focused = true,
                    bounds = TargetBounds(0, 0, 500, 800),
                ),
            ),
            elements = listOf(protectedElement("https://secret.invalid/canary")),
            truncated = false,
        )

        val resolution = resolveVisibleBrowserSurface(
            observation = observation,
            browserPackages = setOf("browser.package"),
            previous = null,
        )

        assertTrue(resolution is BrowserSurfaceResolution.Success)
        val snapshot = (resolution as BrowserSurfaceResolution.Success).snapshot
        assertNull(snapshot.observedUrl)
        assertFalse(snapshot.toString().contains("https://secret.invalid/canary"))
        with(snapshot.elements.single()) {
            assertNull(label)
            assertNull(value)
            assertNull(hint)
            assertNull(stateDescription)
        }
    }

    @Test
    fun extractPresentationContainsNoInlinePageOrArtifactPreview() {
        val capture = captureVisibleBrowserText(listOf(ordinaryElement("visible body")))
        val presentation = presentation(capture, includePreview = false)

        assertFalse(presentation.page.containsKey("text"))
        assertFalse(presentation.page.containsKey("truncated"))
        assertFalse(presentation.artifact.containsKey("preview"))
        assertTrue(
            presentation.completionProof
                .getValue("partial")
                .jsonArray
                .isEmpty(),
        )
        assertFalse(presentation.toString().contains("visible body"))
    }

    private fun presentation(
        capture: VisibleBrowserText,
        includePreview: Boolean,
    ): BrowserCapturePresentation = buildBrowserCapturePresentation(
        title = "Visible title",
        observedUrl = "https://example.invalid/page",
        observationTruncated = false,
        capture = capture,
        artifactInfo = buildJsonObject {
            put("id", "artifact-safe-id")
            put("sha256", capture.text.toByteArray().sha256())
        },
        captureSha256 = capture.text.toByteArray().sha256(),
        includePreview = includePreview,
    )

    private fun protectedElement(value: String) = element(value, isProtected = true)

    private fun ordinaryElement(value: String) = element(
        value = value,
        isProtected = false,
    )

    private fun element(value: String, isProtected: Boolean) = AccessibilityElement(
        id = 1,
        role = "text_field",
        label = if (isProtected) value else "Safe field description",
        value = value,
        hint = if (isProtected) value else "Safe hint",
        stateDescription = if (isProtected) value else "Safe state",
        viewId = null,
        packageName = "browser.package",
        className = "android.widget.EditText",
        bounds = BOUNDS,
        actions = emptySet(),
        enabled = true,
        visible = true,
        focused = false,
        selected = false,
        checked = null,
        isProtected = isProtected,
        controllerOwned = false,
        target = TARGET,
    )

    private companion object {
        const val PROTECTED_CANARY = "canary-browser-password-a41c"
        val BOUNDS = TargetBounds(0, 0, 100, 50)
        val TARGET = PhoneControlTargetIdentity(
            snapshotGeneration = 4,
            displayId = 0,
            windowId = 9,
            packageOrSurface = "browser.package",
            nodeOrDocumentIdentity = "9:1",
            bounds = BOUNDS,
            observationTimestampMs = 10,
        )
    }
}

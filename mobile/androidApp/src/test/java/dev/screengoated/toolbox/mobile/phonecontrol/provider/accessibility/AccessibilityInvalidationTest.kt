package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.view.accessibility.AccessibilityEvent
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import org.junit.Assert.assertFalse
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AccessibilityInvalidationTest {
    @Test
    fun `coordinate dispatch accepts only its exact visual revision`() {
        assertNull(visualRevisionFailure(expected = null, current = 8))
        assertNull(visualRevisionFailure(expected = 8, current = 8))

        val stale = requireNotNull(visualRevisionFailure(expected = 7, current = 8))
        assertEquals("stale_frame", stale.code)
        assertTrue(stale.retryable)
        assertTrue(stale.freshObservationRequired)
    }

    @Test
    fun `window topology and explicit user mutations invalidate immediately`() {
        val eventTypes = listOf(
            AccessibilityEvent.TYPE_WINDOWS_CHANGED,
            AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED,
            AccessibilityEvent.TYPE_VIEW_CLICKED,
            AccessibilityEvent.TYPE_VIEW_SCROLLED,
            AccessibilityEvent.TYPE_VIEW_TEXT_CHANGED,
        )

        eventTypes.forEach { eventType ->
            assertTrue(
                accessibilityInvalidationImpact(eventType, 0) ==
                    AccessibilityInvalidationImpact.HARD,
            )
        }
    }

    @Test
    fun `semantic churn does not retire otherwise live action leases`() {
        val semanticContent = AccessibilityEvent.CONTENT_CHANGE_TYPE_TEXT or
            AccessibilityEvent.CONTENT_CHANGE_TYPE_CONTENT_DESCRIPTION or
            AccessibilityEvent.CONTENT_CHANGE_TYPE_STATE_DESCRIPTION

        assertTrue(
            accessibilityInvalidationImpact(
                AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED,
                semanticContent,
            ) == AccessibilityInvalidationImpact.SEMANTIC_ONLY,
        )
        assertTrue(
            accessibilityInvalidationImpact(
                AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED,
                AccessibilityEvent.CONTENT_CHANGE_TYPE_SUBTREE,
            ) == AccessibilityInvalidationImpact.SEMANTIC_ONLY,
        )
        assertTrue(
            accessibilityInvalidationImpact(
                AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED,
                AccessibilityEvent.CONTENT_CHANGE_TYPE_UNDEFINED,
            ) == AccessibilityInvalidationImpact.SEMANTIC_ONLY,
        )
        listOf(
            AccessibilityEvent.TYPE_VIEW_FOCUSED,
            AccessibilityEvent.TYPE_VIEW_SELECTED,
            AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED,
        ).forEach { eventType ->
            assertTrue(
                accessibilityInvalidationImpact(eventType, 0) ==
                    AccessibilityInvalidationImpact.SEMANTIC_ONLY,
            )
        }
    }

    @Test
    fun `only an exact known non-application controller overlay is ignored`() {
        val windows = listOf(
            window(id = 11, type = "system", packageName = SGT, controllerOwned = true),
            window(id = 12, type = "application", packageName = SGT, controllerOwned = true),
            window(id = 13, type = "accessibility_overlay", packageName = OTHER, controllerOwned = true),
        )

        assertTrue(isKnownControllerOverlayEvent(11, SGT, SGT, windows))
        assertFalse(isKnownControllerOverlayEvent(12, SGT, SGT, windows))
        assertFalse(isKnownControllerOverlayEvent(13, OTHER, SGT, windows))
        assertFalse(isKnownControllerOverlayEvent(13, SGT, SGT, windows))
        assertFalse(isKnownControllerOverlayEvent(-1, SGT, SGT, windows))
        assertFalse(isKnownControllerOverlayEvent(99, SGT, SGT, windows))
    }

    @Test
    fun `unbound controller window events are ignored only during an overlay transition`() {
        assertTrue(
            shouldIgnoreControllerOverlayEvent(
                eventWindowId = -1,
                eventPackage = SGT,
                servicePackage = SGT,
                windows = emptyList(),
                controllerTransitionActive = true,
            ),
        )
        assertFalse(
            shouldIgnoreControllerOverlayEvent(
                eventWindowId = -1,
                eventPackage = SGT,
                servicePackage = SGT,
                windows = emptyList(),
                controllerTransitionActive = false,
            ),
        )
        assertFalse(
            shouldIgnoreControllerOverlayEvent(
                eventWindowId = -1,
                eventPackage = OTHER,
                servicePackage = SGT,
                windows = emptyList(),
                controllerTransitionActive = true,
            ),
        )
    }

    private fun window(
        id: Int,
        type: String,
        packageName: String,
        controllerOwned: Boolean,
    ) = AccessibilityWindowSnapshot(
        id = id,
        displayId = 0,
        layer = 1,
        type = type,
        title = null,
        packageName = packageName,
        active = false,
        focused = false,
        bounds = TargetBounds(0, 0, 100, 100),
        controllerOwned = controllerOwned,
    )

    private companion object {
        const val SGT = "dev.screengoated.toolbox.mobile"
        const val OTHER = "dev.external.overlay"
    }
}

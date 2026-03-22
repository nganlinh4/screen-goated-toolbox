package dev.screengoated.toolbox.mobile.service

import org.junit.Assert.assertEquals
import org.junit.Test

class SgtAccessibilityTextInjectionSupportTest {
    @Test
    fun `append plan inserts text at caret when selection is collapsed`() {
        val plan = buildAccessibilityAppendPlan(
            existingText = "hello world",
            selectionStart = 5,
            selectionEnd = 5,
            appendText = ", there",
        )

        assertEquals("hello, there world", plan.updatedText)
        assertEquals(12, plan.selectionIndex)
    }

    @Test
    fun `append plan replaces selected range`() {
        val plan = buildAccessibilityAppendPlan(
            existingText = "hello world",
            selectionStart = 6,
            selectionEnd = 11,
            appendText = "SGT",
        )

        assertEquals("hello SGT", plan.updatedText)
        assertEquals(9, plan.selectionIndex)
    }

    @Test
    fun `append plan falls back to end when selection is invalid`() {
        val plan = buildAccessibilityAppendPlan(
            existingText = "hello",
            selectionStart = -1,
            selectionEnd = -1,
            appendText = " world",
        )

        assertEquals("hello world", plan.updatedText)
        assertEquals(11, plan.selectionIndex)
    }
}

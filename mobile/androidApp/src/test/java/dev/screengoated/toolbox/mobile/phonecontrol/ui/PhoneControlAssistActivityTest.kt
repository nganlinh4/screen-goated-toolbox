package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.content.Intent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PhoneControlAssistActivityTest {
    @Test
    fun `assist invocation enters only the required structural state`() {
        assertRoute(
            PhoneControlAssistantInvocationRoute.ACTIVATE,
            running = false,
            captureSuspended = false,
        )
        assertRoute(
            PhoneControlAssistantInvocationRoute.PRESERVE_RUNNING,
            running = true,
            captureSuspended = false,
        )
        assertRoute(
            PhoneControlAssistantInvocationRoute.RESUME_CAPTURE,
            running = true,
            captureSuspended = true,
        )
        assertRoute(
            PhoneControlAssistantInvocationRoute.RESUME_CAPTURE,
            running = false,
            captureSuspended = true,
        )
    }

    @Test
    fun `non-assist actions are ignored regardless of runtime state`() {
        listOf(null, Intent.ACTION_MAIN, Intent.ACTION_VIEW).forEach { action ->
            assertEquals(
                PhoneControlAssistantInvocationRoute.IGNORE,
                phoneControlAssistantInvocationRoute(
                    action = action,
                    running = false,
                    captureSuspended = true,
                ),
            )
        }
    }

    @Test
    fun `assist text requires role and stays bounded`() {
        assertEquals(
            "inspect this",
            phoneControlAssistGoal(Intent.ACTION_ASSIST, true, "  inspect this  "),
        )
        assertNull(phoneControlAssistGoal(Intent.ACTION_ASSIST, false, "inspect this"))
        assertNull(
            phoneControlAssistGoal(Intent.ACTION_VIEW, true, "inspect this"),
        )
        assertNull(
            phoneControlAssistGoal(Intent.ACTION_ASSIST, true, "x".repeat(1_025)),
        )
    }

    private fun assertRoute(
        expected: PhoneControlAssistantInvocationRoute,
        running: Boolean,
        captureSuspended: Boolean,
    ) {
        assertEquals(
            expected,
            phoneControlAssistantInvocationRoute(
                action = Intent.ACTION_ASSIST,
                running = running,
                captureSuspended = captureSuspended,
            ),
        )
        assertEquals(expected.name.lowercase(), expected.wireName)
    }
}

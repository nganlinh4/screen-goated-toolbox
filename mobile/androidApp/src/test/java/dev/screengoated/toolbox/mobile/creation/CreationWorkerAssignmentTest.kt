package dev.screengoated.toolbox.mobile.creation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationWorkerAssignmentTest {
    private val firstSink: (String, CreationWorkerEvent) -> Unit = { _, _ -> }
    private val secondSink: (String, CreationWorkerEvent) -> Unit = { _, _ -> }

    @Test
    fun `stale callback cannot release or own a newer assignment`() {
        val guard = CreationWorkerAssignmentGuard()
        guard.claim("old", firstSink)
        assertNotNull(guard.release("old"))
        guard.claim("new", secondSink)

        assertNull(guard.release("old"))
        assertFalse(guard.owns("old", firstSink))
        assertTrue(guard.owns("new", secondSink))
        assertEquals("new", guard.jobId)
    }

    @Test
    fun `worker loss drains the active assignment exactly once`() {
        val guard = CreationWorkerAssignmentGuard()
        guard.claim("job", firstSink)

        assertEquals("job", guard.lose()?.jobId)
        assertNull(guard.lose())
        assertNull(guard.jobId)
    }
}

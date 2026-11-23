package dev.screengoated.toolbox.mobile.creation

import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationSvgSaveTest {
    @Test
    fun `failed persistence reports error and the same draft can be retried`() = runTest {
        val draft = "<svg><path d='M0 0'/></svg>"
        val failure = IOException("storage unavailable")
        val errors = mutableListOf<Throwable>()
        var saved = ""
        saveCreationSvgDraft({ draft }, { throw failure }, { errors.add(it) })
        assertSame(failure, errors.single())
        assertEquals("", saved)
        saveCreationSvgDraft({ draft }, { saved = it }, { errors.add(it) })
        assertEquals(draft, saved)
        assertEquals(1, errors.size)
    }

    @Test
    fun `empty serialization does not invoke persistence`() = runTest {
        val errors = mutableListOf<Throwable>()
        var wrote = false
        saveCreationSvgDraft({ "" }, { wrote = true }, { errors.add(it) })
        assertTrue(!wrote)
        assertTrue(errors.single() is IllegalStateException)
    }

    @Test
    fun `cancellation is not reported as a failed save`() = runTest {
        val cancelled = CancellationException("closed")
        val errors = mutableListOf<Throwable>()
        val thrown = try {
            saveCreationSvgDraft({ throw cancelled }, {}, { errors.add(it) })
            null
        } catch (error: CancellationException) {
            error
        }
        assertSame(cancelled, thrown)
        assertTrue(errors.isEmpty())
    }
}

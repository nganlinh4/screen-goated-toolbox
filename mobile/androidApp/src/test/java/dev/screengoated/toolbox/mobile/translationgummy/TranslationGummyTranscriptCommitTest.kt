package dev.screengoated.toolbox.mobile.translationgummy

import java.util.concurrent.Executor
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TranslationGummyTranscriptCommitTest {
    private fun item(id: Long, text: String, lang: String = "") = TranslationGummyTranscriptItem(
        id = id, role = TranslationGummyTranscriptRole.INPUT, text = text, isFinal = true, updatedAtMs = id, lang = lang,
    )

    @Test
    fun delayedClassificationCannotOverwriteAChangedTranscriptSnapshot() {
        val original = listOf(item(1, "hello"))
        val state = MutableStateFlow(TranslationGummyState(transcripts = original))
        val prepared = PreparedGummyTranscript(original, original + item(2, "world"), index = 1)

        // A clear (or another capture) lands while the language is being classified off-lock.
        state.value = state.value.copy(transcripts = emptyList())

        assertFalse(commitPreparedGummyTranscript(state, prepared, "eng", 200) { it })
        assertEquals(emptyList<TranslationGummyTranscriptItem>(), state.value.transcripts)
    }

    @Test
    fun unchangedSnapshotCommitsWithTheClassifiedLanguageAndCap() {
        val original = listOf(item(1, "a"), item(2, "b"))
        val state = MutableStateFlow(TranslationGummyState(transcripts = original))
        val prepared = PreparedGummyTranscript(original, original + item(3, "c"), index = 2)

        assertTrue(commitPreparedGummyTranscript(state, prepared, "vie", 2) { it })
        assertEquals(listOf(item(2, "b"), item(3, "c", lang = "vie")), state.value.transcripts)
    }

    @Test
    fun burstOfRequestsCoalescesIntoOneWriteOfTheLatestSnapshot() {
        val queued = ArrayDeque<Runnable>()
        val executor = Executor { queued.addLast(it) }
        var latest = 0
        val saved = mutableListOf<Int>()
        val writer = CoalescingSnapshotWriter(executor, { latest }, { saved += it }, {})

        repeat(5) { latest = it; writer.request() }

        assertEquals(1, queued.size)
        queued.removeFirst().run()
        assertEquals(listOf(4), saved)
    }

    @Test
    fun failedWriteStaysPendingAndTheNextRequestRetriesTheLatestSnapshot() {
        val queued = ArrayDeque<Runnable>()
        val executor = Executor { queued.addLast(it) }
        var latest = "first"
        val attempts = AtomicInteger()
        val saved = mutableListOf<String>()
        val failures = mutableListOf<Throwable>()
        val writer = CoalescingSnapshotWriter(executor, { latest }, { value ->
            if (attempts.getAndIncrement() == 0) error("disk full")
            saved += value
        }, { failures += it })

        writer.request()
        queued.removeFirst().run()
        assertEquals(1, failures.size)
        assertTrue(saved.isEmpty())

        latest = "second"
        writer.request()
        queued.removeFirst().run()
        assertEquals(listOf("second"), saved)
        assertTrue(queued.isEmpty())
    }
}

package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.coroutines.ExperimentalForInheritanceCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalForInheritanceCoroutinesApi::class)
class CreationSubmissionPublicationTest {
    private val queued = CreationNativeItem("current", "batch", "source.png", "source.png",
        submitted = true, stage = CreationNativeStage.QUEUED, submissionToken = "token")
    private val published = queued.copy(id = "prior", submissionToken = "old", stage = CreationNativeStage.DONE,
        status = CreationJobStatus(jobId = "prior-job", stage = "done", progressText = "", outputPath = "prior.svg"))
    private val accepted = CreationJobStatus(jobId = "job", stage = "queued", progressText = "")
    private fun state(item: CreationNativeItem = queued) = MutableStateFlow(
        CreationNativeUiState(items = listOf(published, item), selectedItemId = item.id))

    @Test
    fun failedPublicationCompareAndSetCannotReportAnAcceptedJob() {
        val backing = state()
        var interfere = true
        val racing = object : MutableStateFlow<CreationNativeUiState> by backing {
            override fun compareAndSet(expect: CreationNativeUiState, update: CreationNativeUiState): Boolean {
                if (interfere) {
                    interfere = false
                    backing.value = backing.value.copy(items = listOf(published, queued.copy(stage = CreationNativeStage.CANCELLED)))
                }
                return backing.compareAndSet(expect, update)
            }
        }
        assertFalse(publishCreationSubmission(racing, queued, queued, accepted))
        assertEquals(CreationNativeStage.CANCELLED, backing.value.selectedItem?.stage)
        assertEquals(published, backing.value.items.first())
    }

    @Test
    fun acceptedTerminalStatusIsPublishedWithoutPaintingItRunning() {
        val current = state()
        val done = accepted.copy(stage = "done", outputPath = "committed.svg")
        assertTrue(publishCreationSubmission(current, queued, queued, done))
        assertEquals(done, current.value.selectedItem?.status)
        assertEquals(CreationNativeStage.DONE, current.value.selectedItem?.stage)
    }

    @Test
    fun cancellationBeforeStartReturnsReconcilesOnlyItsExactJobAndSubmission() {
        val fixture = JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .map { File(it, "parity-fixtures/image-to-svg/input-contract.json") }.first(File::isFile).readText())
        val cases = fixture.getJSONArray("cancellationOutcomes")
        for (index in 0 until cases.length()) {
            val row = cases.getJSONObject(index)
            val current = state(queued.copy(stage = CreationNativeStage.CANCELLED))
            val result = accepted.copy(stage = row.getString("stage"),
                outputPath = "committed.svg".takeIf { row.getBoolean("keepsPublishedResult") })
            reconcileRetiredCreationSubmission(current, queued, accepted, { id ->
                assertEquals("job", id); listOf(result)
            }, { throw it })
            assertEquals(result, current.value.selectedItem?.status)
            assertEquals(row.getBoolean("terminal"), current.value.selectedItem?.stage?.isTerminal())
            assertEquals(published, current.value.items.first())
        }
    }

    @Test
    fun lateAcceptedDoneIsKeptWithoutCancellingAnAlreadyPublishedResult() {
        val current = state(queued.copy(stage = CreationNativeStage.CANCELLED))
        val done = accepted.copy(stage = "done", outputPath = "committed.svg")
        reconcileRetiredCreationSubmission(current, queued, done, { fail("terminal job"); emptyList() }, { throw it })
        assertEquals(done, current.value.selectedItem?.status)
    }

    @Test
    fun unconfirmedLateCancellationKeepsTheKnownJobActiveForMonitoring() {
        for (throws in listOf(false, true)) {
            val current = state(queued.copy(stage = CreationNativeStage.CANCELLED))
            var errors = 0
            reconcileRetiredCreationSubmission(current, queued, accepted, {
                if (throws) throw IllegalStateException("unavailable") else emptyList()
            }, { errors++ })
            assertEquals(1, errors)
            assertEquals("job", current.value.selectedItem?.status?.jobId)
            assertEquals("cancelling", current.value.selectedItem?.status?.stage)
            assertTrue(creationSurfaceHasActiveWork(current.value.items))
        }
    }

    @Test
    fun replacementSubmissionAndNewerTerminalResultRemainUntouched() {
        for (replacement in listOf(queued.copy(submissionToken = "new"),
            queued.copy(stage = CreationNativeStage.DONE, status = accepted.copy(stage = "done", outputPath = "new.svg")))) {
            val current = state(replacement)
            var calls = 0
            reconcileRetiredCreationSubmission(current, queued, accepted,
                { calls++; listOf(accepted.copy(stage = "cancelled")) }, { throw it })
            assertEquals(1, calls)
            assertEquals(replacement, current.value.selectedItem)
            assertEquals(published, current.value.items.first())
        }
    }
}

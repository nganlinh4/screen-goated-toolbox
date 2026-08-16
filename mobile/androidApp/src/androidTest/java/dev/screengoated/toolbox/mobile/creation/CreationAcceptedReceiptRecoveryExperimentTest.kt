package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.content.Intent
import android.os.SystemClock
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.Base64
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CreationAcceptedReceiptRecoveryExperimentTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()

    @Test
    fun acceptedReceiptRecoversThroughAValidatedHostJournal() {
        assumeTrue(
            "Image creation restoration case is not released",
            creationToolReleased(CreationTool.IMAGE_CREATOR),
        )
        val arguments = InstrumentationRegistry.getArguments()
        val expectedFingerprint = arguments.required("recoveryRequestFingerprint")
        val acceptedAtRange = arguments.required("recoveryAcceptedAtStartMs").toLong()..
            arguments.required("recoveryAcceptedAtEndMs").toLong()
        val request = acceptedAtRange.firstNotNullOfOrNull { acceptedAtMs ->
            val unsigned = CreationWorkerRequest(
                jobId = arguments.required("recoveryJobId"),
                acceptedAtMs = acceptedAtMs,
                deadlineAtMs = acceptedAtMs + CreationContract.MAXIMUM_JOB_RUNTIME_MS,
                dispatchId = arguments.required("recoveryDispatchId"),
                sourceDescriptors = emptyList(),
                tool = CreationTool.IMAGE_CREATOR.wireName,
                operation = CreationContract.IMAGE_CREATOR_OPERATION,
                imagePath = "",
                prompt = String(
                    Base64.getUrlDecoder().decode(arguments.required("recoveryPromptBase64")),
                    Charsets.UTF_8,
                ),
                outputPath = arguments.required("recoveryOutputPath"),
                outputName = arguments.required("recoveryOutputName"),
            )
            unsigned.copy(requestFingerprint = creationRequestFingerprint(unsigned))
                .takeIf { it.requestFingerprint == expectedFingerprint }
        }
        assertNotNull("No exact accepted request matched the bounded admission window", request)
        requireNotNull(request)
        assertEquals(expectedFingerprint, request.requestFingerprint)
        assertFalse(CreationCancellationStore(context).isCancelled(request))
        val files = CreationFileStore(context)
        val expectedOutput = File(request.outputPath)
        if (!expectedOutput.isFile) {
            assertEquals(
                expectedOutput.absolutePath,
                files.stagingFile(CreationTool.IMAGE_CREATOR, "", "png").absolutePath,
            )
        }
        assertTrue(
            validateRestoredCreationRequest(
                context.filesDir,
                request,
                files::size,
                files::sha256,
            ),
        )
        val status = CreationJobFactory.initialStatus(CreationTool.IMAGE_CREATOR, request).copy(
            stage = "generating",
            progressText = "Recovering accepted image",
            phase = "recovering",
            progressRatio = 0.72,
        )
        val record = CreationJournalRecord(
            ownerId = arguments.required("recoveryOwnerId"),
            request = request,
            status = status,
            startedAtMs = request.acceptedAtMs,
            engineId = arguments.required("recoveryEngineId"),
        )
        assertTrue(restoredCreationRecordIsBounded(record, System.currentTimeMillis()))
        CreationJobJournal(context).save(listOf(record))

        val intent = Intent(context, CreationMiniAppActivity::class.java)
            .putExtra("creation_tool", CreationTool.IMAGE_CREATOR.wireName)
            .putExtra("creation_owner_id", record.ownerId)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)
        ActivityScenario.launch<CreationMiniAppActivity>(intent).use {
            val manager = CreationJobManager.get(context)
            val deadline = SystemClock.elapsedRealtime() + RECOVERY_TIMEOUT_MS
            var terminal: CreationJobStatus? = null
            while (SystemClock.elapsedRealtime() < deadline) {
                terminal = manager.statuses(record.ownerId, CreationTool.IMAGE_CREATOR)
                    .firstOrNull { it.dispatchId == request.dispatchId }
                    ?.takeUnless { creationStageIsBusy(it.stage) }
                if (terminal != null) break
                Thread.sleep(POLL_INTERVAL_MS)
            }

            assertNotNull("Accepted creation did not reach a terminal state", terminal)
            assertEquals("Accepted creation did not recover", "done", terminal?.stage)
            val history = manager.history.list(CreationTool.IMAGE_CREATOR)
                .firstOrNull { it.dispatchId == request.dispatchId }
            assertNotNull("Recovered creation did not commit history", history)
            val output = File(requireNotNull(history).outputPath)
            assertTrue("Recovered creation artifact is missing", output.isFile)
            CreationArtifactValidator.validatePng(output, null, null)
        }
    }

    private fun android.os.Bundle.required(name: String): String =
        requireNotNull(getString(name)?.takeIf(String::isNotBlank)) { "$name is required" }

    private companion object {
        const val RECOVERY_TIMEOUT_MS = 15 * 60 * 1_000L
        const val POLL_INTERVAL_MS = 500L
    }
}

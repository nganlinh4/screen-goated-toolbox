package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.content.Intent
import android.os.SystemClock
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CreationAcceptedJobRecoveryTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()

    @Test
    fun acceptedJobRecoversWithoutAReplacementSubmission() {
        val accepted = CreationJobJournal(context).load()
            .filter { creationStageIsBusy(it.status.stage) }
            .single()
        val tool = requireNotNull(CreationTool.fromWireName(accepted.request.tool))
        val intent = Intent(context, CreationMiniAppActivity::class.java)
            .putExtra("creation_tool", tool.wireName)
            .putExtra("creation_owner_id", accepted.ownerId)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)

        ActivityScenario.launch<CreationMiniAppActivity>(intent).use {
            val manager = CreationJobManager.get(context)
            val deadline = SystemClock.elapsedRealtime() + RECOVERY_TIMEOUT_MS
            var terminal: CreationJobStatus? = null
            while (SystemClock.elapsedRealtime() < deadline) {
                terminal = manager.statuses(accepted.ownerId, tool)
                    .firstOrNull { it.dispatchId == accepted.request.dispatchId }
                    ?.takeUnless { creationStageIsBusy(it.stage) }
                if (terminal != null) break
                Thread.sleep(POLL_INTERVAL_MS)
            }

            assertNotNull("Accepted creation did not reach a terminal state", terminal)
            assertEquals("Accepted creation did not recover", "done", terminal?.stage)
            val history = manager.history.list(tool)
                .firstOrNull { it.dispatchId == accepted.request.dispatchId }
            assertNotNull("Recovered creation did not commit history", history)
            val output = File(requireNotNull(history).outputPath)
            assertTrue("Recovered creation artifact is missing", output.isFile)
            when (tool) {
                CreationTool.IMAGE_TO_3D -> CreationArtifactValidator.validateGlb(output)
                CreationTool.IMAGE_TO_SVG -> CreationArtifactValidator.validateSvg(output)
                CreationTool.IMAGE_CREATOR ->
                    CreationArtifactValidator.validatePng(output, null, null)
            }
        }
    }

    private companion object {
        const val RECOVERY_TIMEOUT_MS = 15 * 60 * 1_000L
        const val POLL_INTERVAL_MS = 500L
    }
}

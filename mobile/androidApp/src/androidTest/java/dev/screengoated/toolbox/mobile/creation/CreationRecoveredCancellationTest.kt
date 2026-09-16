package dev.screengoated.toolbox.mobile.creation

import android.content.Intent
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

class CreationRecoveredCancellationTest {
    @get:Rule val compose = createEmptyComposeRule()

    @Test fun cancelExplicitRecoveredJobThroughUi() {
        val jobId = InstrumentationRegistry.getArguments().getString("recoveredJobId")
        assumeTrue("An exact test-owned job is required", !jobId.isNullOrBlank())
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val journal = CreationJobJournal(context)
        val record = journal.load().single { it.request.jobId == jobId }
        val intent = Intent(context, CreationMiniAppActivity::class.java)
            .putExtra("creation_tool", record.request.tool)
            .putExtra("creation_owner_id", record.ownerId)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)
        ActivityScenario.launch<CreationMiniAppActivity>(intent).use {
            compose.waitUntil(30_000) {
                compose.onAllNodesWithTag("creation-cancel-action").fetchSemanticsNodes().isNotEmpty()
            }
            compose.onNodeWithTag("creation-cancel-action").performClick()
            compose.waitUntil(30_000) {
                journal.load().none { it.request.jobId == jobId && creationStageIsBusy(it.status.stage) }
            }
            journal.load().singleOrNull { it.request.jobId == jobId }?.let {
                assertEquals("cancelled", it.status.stage)
            }
        }
    }
}

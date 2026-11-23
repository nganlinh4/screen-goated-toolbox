package dev.screengoated.toolbox.mobile.phonecontrol.authorization

import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.NioTestFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.PreparedFileTarget
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.CompositePhoneControlTurnRecorder
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlUiGoalFlush
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlUiGoalPresentation
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlUserInterfaceGoalQueue
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import java.io.File
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class PhoneControlTextGoalRequestTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    @Test
    fun `typed goal is the committed request of its turn`() = runTest {
        val file = temporaryFolder.newFile("output.txt").apply { writeText("private-old") }
        val contexts = mutableListOf<String>()
        val authorization = PhoneControlResourceAuthorization(
            modelIds = { listOf("first", "second") },
            candidateClient = object : RequestContractCandidateClient {
                override suspend fun request(modelId: String, instruction: String, context: String): Result<String> {
                    contexts += context
                    return Result.success("""{"authorized":true,"reason":"target is in scope"}""")
                }
            },
        )
        // The recorders receive the goal through the session composite, as the turn coordinator sends it.
        val recorder = CompositePhoneControlTurnRecorder(listOf(authorization))
        recorder.turnStarted(1, 1)
        recorder.userRequestSubmitted(1, "Update the designated output file.")

        val decision = authorization.evaluate("edit_text_file", editArgs(file))

        assertTrue(decision.authorized)
        assertTrue(contexts.first().contains("Update the designated output file."))
    }

    @Test
    fun `only a user-authored goal is reported as a request`() {
        val queue = PhoneControlUserInterfaceGoalQueue()
        queue.offer("Open the report.", runtimeReady = true)
        assertEquals(PhoneControlUiGoalFlush.SENT, queue.flush(PhoneControlTurnPhase.IDLE, 0, false) { true })
        assertEquals("Open the report.", queue.inFlightUserRequest())

        val silent = PhoneControlUserInterfaceGoalQueue()
        silent.offer("Navigate setup.", runtimeReady = true, presentation = PhoneControlUiGoalPresentation.SILENT)
        assertEquals(PhoneControlUiGoalFlush.SENT, silent.flush(PhoneControlTurnPhase.IDLE, 0, false) { true })
        assertNull(silent.inFlightUserRequest())
    }

    private fun editArgs(file: File): PreparedResourceMutation =
        PreparedFileTarget.prepare(file.absolutePath, 32 * 1024 * 1024, NioTestFileTransactionIo(), requireExisting = true)
            .use { PreparedResourceMutation("edit_text_file", it) }
}

package dev.screengoated.toolbox.mobile.shared.live

import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class LiveTranslateParityFixturesTest {
    private val json = Json { ignoreUnknownKeys = false }

    @Test
    fun sharedFixturesMatchMobileParityReducer() {
        val fixtures = loadFixtureDocument()
        assertEquals(1, fixtures.version)

        fixtures.cases.forEach { fixtureCase ->
            var state = LiveTranslateParity.reset()
            fixtureCase.steps.forEach { step ->
                state = when (step.type) {
                    "appendTranscript" -> LiveTranslateParity.appendTranscript(
                        state = state,
                        newText = step.text.orEmpty(),
                        nowMs = step.atMs ?: 0L,
                    )

                    "claimTranslationRequest" -> {
                        val (nextState, request) = LiveTranslateParity.claimTranslationRequest(state)
                        assertRequest(step.expectedRequest, request)
                        nextState
                    }

                    "appendTranslation" -> LiveTranslateParity.appendTranslationDelta(
                        state = state,
                        newText = step.text.orEmpty(),
                        nowMs = step.atMs ?: 0L,
                    )

                    "finalizeTranslation" -> LiveTranslateParity.finalizeTranslation(
                        state = state,
                        bytesToCommit = step.bytesToCommit ?: 0,
                    )

                    "forceCommitAll" -> LiveTranslateParity.forceCommitAll(state)
                    "clearTranslationHistory" -> LiveTranslateParity.clearTranslationHistory(state)
                    else -> error("Unknown fixture step type: ${step.type}")
                }
            }

            assertState(fixtureCase.expectedState, state)
        }
    }

    @Test
    fun forceCommitTimeoutMatchesWindowsGeminiThresholds() {
        var state = LiveTranslateParity.reset()
        state = LiveTranslateParity.appendTranscript(state, "hello world", 100L)
        val (requestedState, request) = LiveTranslateParity.claimTranslationRequest(state)
        state = requestedState
        assertNotNull(request)
        assertFalse(request.hasFinishedDelimiter)

        state = LiveTranslateParity.appendTranslationDelta(state, "hola mundo", 200L)

        val beforeThreshold = LiveTranslateParity.forceCommitIfDue(state, 1_100L)
        assertFalse(beforeThreshold.second)

        val afterThreshold = LiveTranslateParity.forceCommitIfDue(state, 1_300L)
        assertTrue(afterThreshold.second)
        assertEquals("hola mundo", afterThreshold.first.committedTranslation)
        assertEquals("hello world", afterThreshold.first.translationHistory.single().source)
    }

    private fun assertRequest(
        expected: FixtureExpectedRequest?,
        actual: TranslationRequest?,
    ) {
        if (expected == null) {
            assertEquals(null, actual)
            return
        }

        assertNotNull(actual)
        assertEquals(expected.chunk, actual.chunk)
        assertEquals(expected.hasFinishedDelimiter, actual.hasFinishedDelimiter)
        assertEquals(expected.bytesToCommit, actual.bytesToCommit)
    }

    private fun assertState(
        expected: FixtureExpectedState,
        actual: LiveTextState,
    ) {
        assertEquals(expected.fullTranscript, actual.fullTranscript)
        assertEquals(expected.displayTranscript, actual.displayTranscript)
        assertEquals(expected.lastCommittedPos, actual.lastCommittedPos)
        assertEquals(expected.lastProcessedLen, actual.lastProcessedLen)
        assertEquals(expected.committedTranslation, actual.committedTranslation)
        assertEquals(expected.uncommittedTranslation, actual.uncommittedTranslation)
        assertEquals(expected.displayTranslation, actual.displayTranslation)
        assertEquals(expected.translationHistory, actual.translationHistory)
    }

    private fun loadFixtureDocument(): FixtureDocument {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val repoRoot = generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")

        val fixtureFile = File(repoRoot, FIXTURE_PATH)
        return json.decodeFromString(fixtureFile.readText())
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/live-translate/state-machine.json"
    }
}

@Serializable
private data class FixtureDocument(
    val version: Int,
    val cases: List<FixtureCase>,
)

@Serializable
private data class FixtureCase(
    val name: String,
    val steps: List<FixtureStep>,
    val expectedState: FixtureExpectedState,
)

@Serializable
private data class FixtureStep(
    val type: String,
    val text: String? = null,
    val atMs: Long? = null,
    val bytesToCommit: Int? = null,
    val expectedRequest: FixtureExpectedRequest? = null,
)

@Serializable
private data class FixtureExpectedRequest(
    val chunk: String,
    val hasFinishedDelimiter: Boolean,
    val bytesToCommit: Int,
)

@Serializable
private data class FixtureExpectedState(
    val fullTranscript: String,
    val displayTranscript: String,
    val lastCommittedPos: Int,
    val lastProcessedLen: Int,
    val committedTranslation: String,
    val uncommittedTranslation: String,
    val displayTranslation: String,
    val translationHistory: List<TranslationHistoryEntry>,
)

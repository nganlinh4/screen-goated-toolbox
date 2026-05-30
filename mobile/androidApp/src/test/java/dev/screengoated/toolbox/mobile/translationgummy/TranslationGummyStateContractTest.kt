package dev.screengoated.toolbox.mobile.translationgummy

import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Test

class TranslationGummyStateContractTest {
    private val json = Json { ignoreUnknownKeys = false }

    @Test
    fun `apply draft preserves transcript history and inserts Windows separator`() {
        val fixture = loadFixtureDocument()
        assertEquals(1, fixture.version)

        fixture.cases.forEach { fixtureCase ->
            val initial = TranslationGummyState(
                appliedConfig = TranslationGummyConfig(),
                draftConfig = TranslationGummyConfig(
                    first = TranslationGummyLanguageProfile(language = "Vietnamese"),
                    second = TranslationGummyLanguageProfile(language = "English"),
                ),
                transcripts = fixtureCase.initial.transcripts,
            )

            val applied = initial.draftConfig.normalized()
            val actual = initial.afterApplyingDraftForWindowsParity(
                applied = applied,
                nextSeparatorId = fixtureCase.apply.nextSeparatorId,
                separatorText = fixtureCase.apply.separatorText,
                nowMs = fixtureCase.apply.nowMs,
            )

            assertEquals(fixtureCase.name, fixtureCase.expected.transcripts, actual.transcripts)
            assertEquals(fixtureCase.name, applied, actual.appliedConfig)
            assertEquals(fixtureCase.name, applied, actual.draftConfig)
        }
    }

    private fun loadFixtureDocument(): StateFixtureDocument {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val repoRoot = generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")

        return json.decodeFromString(File(repoRoot, FIXTURE_PATH).readText())
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/translation-gummy/state-contract.json"
    }
}

@Serializable
private data class StateFixtureDocument(
    val version: Int,
    val cases: List<StateFixtureCase>,
)

@Serializable
private data class StateFixtureCase(
    val name: String,
    val initial: TranscriptStateSnapshot,
    val apply: ApplySnapshot,
    val expected: TranscriptStateSnapshot,
)

@Serializable
private data class TranscriptStateSnapshot(
    val transcripts: List<TranslationGummyTranscriptItem>,
)

@Serializable
private data class ApplySnapshot(
    val nextSeparatorId: Long,
    val separatorText: String,
    val nowMs: Long,
)

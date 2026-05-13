package dev.screengoated.toolbox.mobile.translationgummy

import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Test

class TranslationGummyVolumeStateTest {
    private val json = Json { ignoreUnknownKeys = false }

    @Test
    fun `volume fixture matches mobile model`() {
        val fixture = loadFixtureDocument()
        assertEquals(1, fixture.version)
        assertEquals(TranslationGummyVolumeState.DEFAULT_VOLUME_PERCENT, fixture.defaultVolumePercent)
        assertEquals(TranslationGummyVolumeState.MIN_VOLUME_PERCENT, fixture.range.min)
        assertEquals(TranslationGummyVolumeState.MAX_VOLUME_PERCENT, fixture.range.max)
        assertEquals(TranslationGummyVolumeState.STEP_PERCENT, fixture.range.step)

        fixture.cases.forEach { fixtureCase ->
            val actual = fixtureCase.steps.fold(fixtureCase.initial.toState()) { state, step ->
                when (step.type) {
                    "setVolume" -> state.withPercent(step.value ?: error("setVolume requires value"))
                    "toggleMute" -> state.toggleMuted()
                    else -> error("Unknown volume step type: ${step.type}")
                }
            }

            assertEquals(fixtureCase.name, fixtureCase.expected.toState(), actual)
        }
    }

    @Test
    fun `default volume is full output`() {
        assertEquals(100, TranslationGummyVolumeState().percent)
        assertEquals(100, TranslationGummyVolumeState().restorePercent)
    }

    private fun loadFixtureDocument(): VolumeFixtureDocument {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val repoRoot = generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")

        return json.decodeFromString(File(repoRoot, FIXTURE_PATH).readText())
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/translation-gummy/volume-control.json"
    }
}

@Serializable
private data class VolumeFixtureDocument(
    val version: Int,
    val defaultVolumePercent: Int,
    val range: VolumeRange,
    val cases: List<VolumeCase>,
)

@Serializable
private data class VolumeRange(
    val min: Int,
    val max: Int,
    val step: Int,
)

@Serializable
private data class VolumeCase(
    val name: String,
    val initial: VolumeSnapshot,
    val steps: List<VolumeStep>,
    val expected: VolumeSnapshot,
)

@Serializable
private data class VolumeStep(
    val type: String,
    val value: Int? = null,
)

@Serializable
private data class VolumeSnapshot(
    val percent: Int,
    val restorePercent: Int,
) {
    fun toState(): TranslationGummyVolumeState =
        TranslationGummyVolumeState(percent = percent, restorePercent = restorePercent)
}

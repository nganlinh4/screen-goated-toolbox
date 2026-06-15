package dev.screengoated.toolbox.mobile.parity

import dev.screengoated.toolbox.mobile.shared.live.OfflineAsrCommitState
import dev.screengoated.toolbox.mobile.shared.live.OfflineAsrStreamParity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

/**
 * Asserts the Android [OfflineAsrStreamParity] port produces byte-identical output to
 * the Windows-canonical machine, by running the SAME golden fixtures the Rust side
 * asserts (`parity-fixtures/offline-asr-stream/cases.json`). If the two ever drift,
 * one of these test suites goes red. See `.claude/parity/offline-asr-stream.md`.
 */
class OfflineAsrStreamParityTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun matchesSharedGoldenFixtures() {
        val doc = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString()).jsonObject
        val cases = doc["cases"]!!.jsonArray
        for (caseEl in cases) {
            val case = caseEl.jsonObject
            val name = case["name"]!!.jsonPrimitive.content
            val state = OfflineAsrCommitState()
            case["steps"]!!.jsonArray.forEachIndexed { i, stepEl ->
                val step = stepEl.jsonObject
                val active = OfflineAsrStreamParity.commitStep(
                    state,
                    step["text"]!!.jsonPrimitive.content,
                    step["hasNativePunctuation"]!!.jsonPrimitive.boolean,
                    step["nowMs"]!!.jsonPrimitive.int.toLong(),
                )
                assertEquals(
                    "case '$name' step $i history",
                    step["expectCommittedHistory"]!!.jsonPrimitive.content,
                    state.committedHistory,
                )
                assertEquals(
                    "case '$name' step $i draft",
                    step["expectActiveDraft"]!!.jsonPrimitive.content,
                    active,
                )
            }
        }
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "offline-asr-stream", "cases.json"),
            Paths.get("..", "..", "parity-fixtures", "offline-asr-stream", "cases.json"),
            Paths.get("parity-fixtures", "offline-asr-stream", "cases.json"),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing offline-asr-stream fixture. Tried: $candidates")
    }
}

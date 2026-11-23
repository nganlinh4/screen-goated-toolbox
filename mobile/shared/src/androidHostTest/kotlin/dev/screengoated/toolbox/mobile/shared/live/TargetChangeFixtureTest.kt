package dev.screengoated.toolbox.mobile.shared.live

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Paths

class TargetChangeFixtureTest {
    @Test
    fun targetChangeRetriesOnlyTheUntranslatedTail() {
        val paths = listOf(
            Paths.get("..", "parity-fixtures", "live-translate", "target-change.json"),
            Paths.get("..", "..", "parity-fixtures", "live-translate", "target-change.json"),
            Paths.get("parity-fixtures", "live-translate", "target-change.json"),
        )
        val fixture = Json.parseToJsonElement(Files.readAllBytes(paths.first { Files.exists(it) }).decodeToString()).jsonObject
        fixture.getValue("cases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val committed = case.getValue("committed").jsonPrimitive.content
            val draft = case.getValue("draft").jsonPrimitive.content
            val source = committed + draft
            val state = LiveTranslateParity.reset(transcriptionMethod = TranscriptionMethod.GEMINI_LIVE).copy(
                fullTranscript = source,
                lastCommittedPos = committed.length,
                lastProcessedLen = source.length,
                committedTranslation = "existing translation",
            )
            val store = LiveSessionStore(LiveSessionState(liveText = state))
            store.updateConfig(LiveSessionPatch(targetLanguage = "English"))
            val current = store.state.value.liveText
            val name = case.getValue("name").jsonPrimitive.content
            assertEquals(name, case.getValue("requestSource").jsonPrimitive.contentOrNull, LiveTranslateParity.claimTranslationRequest(current)?.pendingSource)
            assertEquals(name, "existing translation", current.committedTranslation)
            assertEquals(name, committed.length, current.lastCommittedPos)
        }
    }
}

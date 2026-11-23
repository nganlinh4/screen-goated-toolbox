package dev.screengoated.toolbox.mobile.shared.live

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Paths

class TranslationRequestIdentityFixtureTest {
    @Test
    fun requestIdentityAndCorrectionRetryMatchSharedContract() {
        val paths = listOf(
            Paths.get("..", "parity-fixtures", "live-translate", "translation-request-identity.json"),
            Paths.get("..", "..", "parity-fixtures", "live-translate", "translation-request-identity.json"),
            Paths.get("parity-fixtures", "live-translate", "translation-request-identity.json"),
        )
        val fixture = Json.parseToJsonElement(Files.readAllBytes(paths.first { Files.exists(it) }).decodeToString()).jsonObject
        fun state(text: String) = LiveTranslateParity.setTranscriptSegments(
            LiveTranslateParity.reset(transcriptionMethod = TranscriptionMethod.GEMINI_LIVE), text, "", 1,
        )
        fixture.getValue("cases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val initial = state(case.getValue("requested").jsonPrimitive.content)
            val request = requireNotNull(LiveTranslateParity.claimTranslationRequest(initial))
            val current = LiveTranslateParity.setTranscriptSegments(initial, case.getValue("current").jsonPrimitive.content, "", 2)
            val response = TranslationResponse(listOf(TranslationPatch(request.sourceStart, request.finalizedSourceEnd, "final", "translated")))
            val result = LiveTranslateParity.applyTranslationResponse(current, request, response, 3)
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("accepted").jsonPrimitive.boolean, result.committedTranslation == "translated")
        }
        fixture.getValue("revisionCases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val initial = state(case.getValue("requested").jsonPrimitive.content)
            val request = requireNotNull(LiveTranslateParity.claimTranslationRequest(initial))
            val store = LiveSessionStore(LiveSessionState(liveText = initial))
            if (case["reset"]?.jsonPrimitive?.boolean == true) {
                store.freezeCurrentTranscript()
                store.setTranscriptSegments(case.getValue("current").jsonPrimitive.content, "", 2)
            }
            when (case["change"]?.jsonPrimitive?.content) {
                "target" -> store.updateConfig(LiveSessionPatch(targetLanguage = "English"))
                "model" -> store.updateConfig(LiveSessionPatch(translationProvider = ProviderDescriptor("changed", "changed")))
                "source" -> store.updateConfig(LiveSessionPatch(sourceMode = SourceMode.MIC))
                "transcription" -> store.updateConfig(LiveSessionPatch(transcriptionProvider = ProviderDescriptor("changed", "changed")))
                "transcriptionLanguage" -> store.updateConfig(LiveSessionPatch(transcriptionLanguage = "changed"))
                "vocabulary" -> store.updateConfig(LiveSessionPatch(customVocabulary = listOf("example")))
            }
            val current = store.state.value.liveText
            if (case["change"] != null) assertEquals(initial.translationRevision, current.translationRevision)
            val response = TranslationResponse(listOf(TranslationPatch(request.sourceStart, request.finalizedSourceEnd, "final", "translated")))
            val result = LiveTranslateParity.applyTranslationResponse(current, request, response, 3)
            val accepted = case.getValue("accepted").jsonPrimitive.boolean
            assertEquals(case.getValue("name").jsonPrimitive.content, accepted, LiveTranslateParity.isTranslationRequestCurrent(current, request))
            assertEquals(case.getValue("name").jsonPrimitive.content, accepted, result.committedTranslation == "translated")
        }
        fixture.getValue("forceCommitCases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val pending = case.getValue("pending").jsonPrimitive.content
            val initial = state(pending).copy(
                uncommittedTranslation = "translated",
                uncommittedSourceStart = 0,
                uncommittedSourceEnd = pending.length,
                lastTranscriptAppendAtMs = 0,
                lastTranslationUpdateAtMs = 0,
            )
            val (_, committed) = LiveTranslateParity.forceCommitIfDue(initial, 2_000)
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("commit").jsonPrimitive.boolean, committed)
        }
        fixture.getValue("retryCases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val initial = state(case.getValue("processed").jsonPrimitive.content)
            val request = requireNotNull(LiveTranslateParity.claimTranslationRequest(initial))
            val response = TranslationResponse(listOf(TranslationPatch(request.draftSourceStart, request.sourceEnd, "draft", "translated")))
            val translated = LiveTranslateParity.applyTranslationResponse(initial, request, response, 2)
            val current = LiveTranslateParity.setTranscriptSegments(translated, case.getValue("current").jsonPrimitive.content, "", 3)
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("retry").jsonPrimitive.boolean, LiveTranslateParity.claimTranslationRequest(current) != null)
        }
        fixture.getValue("prefixCases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val initial = state(case.getValue("previous").jsonPrimitive.content)
                .let { it.copy(lastCommittedPos = it.fullTranscript.length) }
            val current = LiveTranslateParity.setTranscriptSegments(initial, case.getValue("current").jsonPrimitive.content, "", 2)
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("retained").jsonPrimitive.content, current.fullTranscript.substring(0, current.lastCommittedPos))
        }
        fixture.getValue("finalityCases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val initial = LiveTranslateParity.setTranscriptSegments(
                LiveTranslateParity.reset(transcriptionMethod = TranscriptionMethod.GEMINI_TRANSCRIBE),
                case.getValue("initialCommitted").jsonPrimitive.content,
                case.getValue("initialDraft").jsonPrimitive.content,
                1,
            )
            val request = requireNotNull(LiveTranslateParity.claimTranslationRequest(initial))
            val patches = buildList {
                if (request.bytesToCommit > 0) add(TranslationPatch(request.sourceStart, request.finalizedSourceEnd, "final", "final text"))
                if (request.draftSource.isNotEmpty()) add(TranslationPatch(request.draftSourceStart, request.sourceEnd, "draft", "draft text"))
            }
            val response = TranslationResponse(patches)
            val currentCommitted = case.getValue("currentCommitted").jsonPrimitive.content
            val currentDraft = case.getValue("currentDraft").jsonPrimitive.content
            val current = LiveTranslateParity.setTranscriptSegments(initial, currentCommitted, currentDraft, 2)
            val applied = LiveTranslateParity.applyTranslationResponse(current, request, response, 3)
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("accepted").jsonPrimitive.boolean, applied != current)
            val translated = LiveTranslateParity.applyTranslationResponse(initial, request, response, 2)
            val changed = LiveTranslateParity.setTranscriptSegments(translated, currentCommitted, currentDraft, 3)
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("retry").jsonPrimitive.boolean, LiveTranslateParity.claimTranslationRequest(changed) != null)
        }
    }

    @Test
    fun newSessionRetiresOldRequestEvenWhenTextMatches() {
        val initial = LiveTranslateParity.setTranscriptSegments(
            LiveTranslateParity.reset(transcriptionMethod = TranscriptionMethod.GEMINI_LIVE),
            "Same source.", "", 1,
        )
        val request = requireNotNull(LiveTranslateParity.claimTranslationRequest(initial))
        val store = LiveSessionStore(LiveSessionState(liveText = initial))
        store.markStarting()
        store.setTranscriptSegments("Same source.", "", 2)
        assertEquals(false, LiveTranslateParity.isTranslationRequestCurrent(store.state.value.liveText, request))
    }
}

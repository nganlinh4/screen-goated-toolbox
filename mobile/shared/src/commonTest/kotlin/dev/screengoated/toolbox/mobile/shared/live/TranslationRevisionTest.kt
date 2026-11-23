package dev.screengoated.toolbox.mobile.shared.live

import kotlin.test.Test
import kotlin.test.assertEquals

class TranslationRevisionTest {
    @Test
    fun resetAndFreezeAdvanceTranslationIdentity() {
        var state = LiveTranslateParity.reset(transcriptionMethod = TranscriptionMethod.GEMINI_TRANSCRIBE)
        state = LiveTranslateParity.setTranscriptSegments(state, "Original source.", "", 1)
        state = state.copy(lastCommittedPos = state.fullTranscript.length, committedTranslation = "First")
        state = LiveTranslateParity.setTranscriptSegments(state, "Replacement.", "", 2)
        assertEquals("", state.committedTranslation)
        assertEquals(1L, state.translationRevision)
        val frozen = LiveTranslateParity.freezeCurrentTranscript(state)
        assertEquals(2L, frozen.translationRevision)
    }
}

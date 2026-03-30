package dev.screengoated.toolbox.mobile.shared.live

private const val USER_SILENCE_TIMEOUT_MS = 800L
private const val AI_SILENCE_TIMEOUT_MS = 1000L
private const val MIN_FORCE_COMMIT_CHARS = 10
private const val PARAKEET_BASE_TIMEOUT_MS = 800L
private const val PARAKEET_SHORT_TIMEOUT_MS = 1200L
private const val PARAKEET_MIN_WORDS = 2
private const val PARAKEET_MIN_TIMEOUT_MS = 350L
private const val PARAKEET_TIMEOUT_DECAY_RATE = 2.5
private const val MAX_TRANSLATION_HISTORY = 3

object LiveTranslateParity {
    fun reset(
        nowMs: Long = 0L,
        transcriptionMethod: TranscriptionMethod = TranscriptionMethod.GEMINI_LIVE,
    ): LiveTextState = LiveTextState(
        lastTranscriptAppendAtMs = nowMs,
        lastTranslationUpdateAtMs = nowMs,
        transcriptionMethod = transcriptionMethod,
    )

    fun setTranscriptionMethod(
        state: LiveTextState,
        method: TranscriptionMethod,
    ): LiveTextState = state.copy(transcriptionMethod = method)

    fun appendTranscript(
        state: LiveTextState,
        newText: String,
        nowMs: Long,
    ): LiveTextState {
        if (newText.isEmpty()) {
            return state
        }

        var textToAppend = newText
        if (state.transcriptionMethod == TranscriptionMethod.PARAKEET) {
            val needsCap = state.fullTranscript.trim().isEmpty() || sourceEndsWithSentence(state)
            val firstContentIndex = textToAppend.indexOfFirst { !it.isWhitespace() }
            if (needsCap && firstContentIndex >= 0) {
                val prefix = textToAppend.substring(0, firstContentIndex)
                val firstChar = textToAppend[firstContentIndex].uppercaseChar()
                val suffix = textToAppend.substring(firstContentIndex + 1)
                textToAppend = "$prefix$firstChar$suffix"
            }
        }

        val fullTranscript = state.fullTranscript + textToAppend
        return state.copy(
            fullTranscript = fullTranscript,
            displayTranscript = fullTranscript,
            lastTranscriptAppendAtMs = nowMs,
        )
    }

    fun claimTranslationRequest(state: LiveTextState): TranslationRequest? {
        if (state.fullTranscript.length == state.lastProcessedLen) {
            return null
        }

        val chunk = getTranslationChunk(state) ?: return null
        return TranslationRequest(
            sourceStart = chunk.sourceStart,
            sourceEnd = chunk.sourceEnd,
            finalizedSourceEnd = chunk.finalizedSourceEnd,
            pendingSource = chunk.text,
            finalizedSource = chunk.finalizedSource,
            draftSource = chunk.draftSource,
            previousDraftTranslation = if (
                state.uncommittedSourceStart == chunk.sourceStart &&
                state.uncommittedSourceEnd == chunk.sourceEnd
            ) {
                state.uncommittedTranslation
            } else {
                ""
            },
            history = state.translationHistory,
        )
    }

    fun applyTranslationResponse(
        state: LiveTextState,
        request: TranslationRequest,
        response: TranslationResponse,
        nowMs: Long,
    ): LiveTextState {
        if (request.sourceStart != state.lastCommittedPos || request.sourceEnd > state.fullTranscript.length) {
            return state
        }

        val finalizedPatch = response.patches.firstOrNull { patch ->
            patch.state == "final" &&
                patch.sourceStart == request.sourceStart &&
                patch.sourceEnd == request.finalizedSourceEnd
        }
        val draftPatch = response.patches.firstOrNull { patch ->
            patch.state == "draft" &&
                patch.sourceStart == request.draftSourceStart &&
                patch.sourceEnd == request.sourceEnd
        }

        if (request.bytesToCommit > 0 && finalizedPatch?.translation?.isBlank() != false) {
            return state
        }
        val normalizedDraftTranslation = when {
            draftPatch?.translation?.isBlank() == false -> draftPatch.translation.trim()
            else -> request.fallbackDraftTranslation()
        }

        if (request.requiresDraftTranslation() && normalizedDraftTranslation.isBlank()) {
            return state
        }

        var nextState = state
        if (request.bytesToCommit > 0) {
            val finalizedTranslation = finalizedPatch?.translation.orEmpty().trim()
            val committedTranslation = joinCommittedTranslation(
                committedTranslation = nextState.committedTranslation,
                newSegment = finalizedTranslation,
            )
            nextState = nextState.copy(
                committedTranslation = committedTranslation,
                lastCommittedPos = request.finalizedSourceEnd,
                translationHistory = addToHistory(
                    history = nextState.translationHistory,
                    source = request.finalizedSource.trim(),
                    translation = finalizedTranslation,
                ),
            )
        }

        nextState = if (request.draftSource.isEmpty()) {
            clearUncommittedTranslation(nextState)
        } else {
            nextState.copy(
                uncommittedTranslation = normalizedDraftTranslation,
                uncommittedSourceStart = request.draftSourceStart,
                uncommittedSourceEnd = request.sourceEnd,
                displayTranslation = updateDisplayTranslation(
                    committedTranslation = nextState.committedTranslation,
                    uncommittedTranslation = normalizedDraftTranslation,
                ),
                lastTranslationUpdateAtMs = nowMs,
            )
        }

        return nextState.copy(lastProcessedLen = request.sourceEnd)
    }

    fun clearTranslationHistory(state: LiveTextState): LiveTextState = state.copy(
        translationHistory = emptyList(),
    )

    fun forceCommitIfDue(
        state: LiveTextState,
        nowMs: Long,
    ): Pair<LiveTextState, Boolean> {
        if (!shouldForceCommitOnTimeout(state, nowMs)) {
            return state to false
        }
        return forceCommitAll(state) to true
    }

    private fun getTranslationChunk(state: LiveTextState): TranslationChunk? {
        if (state.lastCommittedPos >= state.fullTranscript.length) {
            return null
        }

        val text = state.fullTranscript.substring(state.lastCommittedPos)
        if (text.isBlank()) {
            return null
        }

        val sourceStart = state.lastCommittedPos
        val sourceEnd = state.fullTranscript.length
        var splitIndex: Int? = null
        for (index in text.indices) {
            if (SENTENCE_DELIMITERS.contains(text[index])) {
                splitIndex = index + 1
            }
        }

        val finalizedLength = splitIndex ?: 0
        val rawFinalizedSource = text.substring(0, finalizedLength)
        val rawDraftSource = text.substring(finalizedLength)
        val hasMeaningfulDraft = rawDraftSource.isNotBlank()
        val finalizedSourceEnd = if (hasMeaningfulDraft) {
            sourceStart + finalizedLength
        } else {
            sourceEnd
        }
        return TranslationChunk(
            sourceStart = sourceStart,
            sourceEnd = sourceEnd,
            finalizedSourceEnd = finalizedSourceEnd,
            text = text,
            finalizedSource = if (hasMeaningfulDraft) rawFinalizedSource else text,
            draftSource = if (hasMeaningfulDraft) rawDraftSource else "",
        )
    }

    private fun shouldForceCommitOnTimeout(
        state: LiveTextState,
        nowMs: Long,
    ): Boolean {
        if (state.transcriptionMethod == TranscriptionMethod.PARAKEET) {
            if (state.lastCommittedPos >= state.fullTranscript.length) {
                return false
            }

            val wordCount = countUncommittedWords(state)
            if (wordCount < PARAKEET_MIN_WORDS) {
                return false
            }

            val userTimeoutMs = calculateParakeetTimeoutMs(state, wordCount)
            return nowMs - state.lastTranscriptAppendAtMs > userTimeoutMs
        }

        if (state.uncommittedTranslation.isBlank()) {
            return false
        }

        val pendingEnd = state.uncommittedSourceEnd.coerceAtMost(state.fullTranscript.length)
        if (pendingEnd <= state.lastCommittedPos) {
            return false
        }
        val pendingLen = pendingEnd - state.lastCommittedPos
        if (pendingLen < MIN_FORCE_COMMIT_CHARS) {
            return false
        }

        val userSilent = nowMs - state.lastTranscriptAppendAtMs > USER_SILENCE_TIMEOUT_MS
        val aiSilent = nowMs - state.lastTranslationUpdateAtMs > AI_SILENCE_TIMEOUT_MS
        val sourceReady = sourceRangeEndsWithSentence(state, pendingEnd) || pendingEnd > state.lastCommittedPos
        return sourceReady && userSilent && aiSilent
    }

    private fun countUncommittedWords(state: LiveTextState): Int {
        if (state.lastCommittedPos >= state.fullTranscript.length) {
            return 0
        }
        return state.fullTranscript
            .substring(state.lastCommittedPos)
            .split(Regex("\\s+"))
            .count { it.isNotBlank() }
    }

    private fun calculateParakeetTimeoutMs(
        state: LiveTextState,
        wordCount: Int,
    ): Long {
        if (wordCount < 5) {
            return PARAKEET_SHORT_TIMEOUT_MS
        }

        val segmentLen = if (state.lastCommittedPos >= state.fullTranscript.length) {
            0
        } else {
            state.fullTranscript.substring(state.lastCommittedPos).length
        }

        val threshold = 30
        if (segmentLen <= threshold) {
            return PARAKEET_BASE_TIMEOUT_MS
        }

        val excessChars = segmentLen - threshold
        val decay = (excessChars * PARAKEET_TIMEOUT_DECAY_RATE).toLong()
        return (PARAKEET_BASE_TIMEOUT_MS - decay).coerceAtLeast(PARAKEET_MIN_TIMEOUT_MS)
    }

    private fun sourceEndsWithSentence(state: LiveTextState): Boolean {
        return sourceRangeEndsWithSentence(state, state.fullTranscript.length)
    }

    private fun sourceRangeEndsWithSentence(
        state: LiveTextState,
        endExclusive: Int,
    ): Boolean {
        if (state.lastCommittedPos >= state.fullTranscript.length) {
            return false
        }
        if (endExclusive <= state.lastCommittedPos || endExclusive > state.fullTranscript.length) {
            return false
        }

        return state.fullTranscript
            .substring(state.lastCommittedPos, endExclusive)
            .trim()
            .lastOrNull()
            ?.let(SENTENCE_DELIMITERS::contains)
            ?: false
    }

    fun forceCommitAll(state: LiveTextState): LiveTextState {
        if (state.transcriptionMethod == TranscriptionMethod.PARAKEET) {
            if (state.lastCommittedPos < state.fullTranscript.length && !sourceEndsWithSentence(state)) {
                val transcript = state.fullTranscript + ". "
                return state.copy(
                    fullTranscript = transcript,
                    displayTranscript = transcript,
                )
            }
            return state
        }

        if (state.uncommittedTranslation.isBlank()) {
            return state
        }

        val translatedSegment = state.uncommittedTranslation.trim()
        if (translatedSegment.isEmpty()) {
            return clearUncommittedTranslation(state)
        }

        val pendingEnd = state.uncommittedSourceEnd.coerceAtMost(state.fullTranscript.length)
        val sourceSegment = if (state.lastCommittedPos < pendingEnd) {
            state.fullTranscript.substring(state.lastCommittedPos, pendingEnd).trim()
        } else {
            "[continued]"
        }
        val committedTranslation = joinCommittedTranslation(
            committedTranslation = state.committedTranslation,
            newSegment = translatedSegment,
        )
        return state.copy(
            committedTranslation = committedTranslation,
            uncommittedTranslation = "",
            displayTranslation = updateDisplayTranslation(
                committedTranslation = committedTranslation,
                uncommittedTranslation = "",
            ),
            lastCommittedPos = pendingEnd,
            uncommittedSourceStart = pendingEnd,
            uncommittedSourceEnd = pendingEnd,
            translationHistory = addToHistory(
                history = state.translationHistory,
                source = sourceSegment,
                translation = translatedSegment,
            ),
        )
    }

    private fun addToHistory(
        history: List<TranslationHistoryEntry>,
        source: String,
        translation: String,
    ): List<TranslationHistoryEntry> {
        val nextHistory = (history + TranslationHistoryEntry(source = source, translation = translation))
        return if (nextHistory.size <= MAX_TRANSLATION_HISTORY) {
            nextHistory
        } else {
            nextHistory.takeLast(MAX_TRANSLATION_HISTORY)
        }
    }

    private fun joinCommittedTranslation(
        committedTranslation: String,
        newSegment: String,
    ): String {
        return if (committedTranslation.isBlank()) {
            newSegment
        } else {
            "$committedTranslation $newSegment"
        }
    }

    private fun updateDisplayTranslation(
        committedTranslation: String,
        uncommittedTranslation: String,
    ): String {
        return when {
            committedTranslation.isBlank() -> uncommittedTranslation
            uncommittedTranslation.isBlank() -> committedTranslation
            else -> "$committedTranslation $uncommittedTranslation"
        }
    }

    private fun clearUncommittedTranslation(state: LiveTextState): LiveTextState {
        return state.copy(
            uncommittedTranslation = "",
            uncommittedSourceStart = state.lastCommittedPos,
            uncommittedSourceEnd = state.lastCommittedPos,
            displayTranslation = updateDisplayTranslation(
                committedTranslation = state.committedTranslation,
                uncommittedTranslation = "",
            ),
        )
    }

    private data class TranslationChunk(
        val sourceStart: Int,
        val sourceEnd: Int,
        val finalizedSourceEnd: Int,
        val text: String,
        val finalizedSource: String,
        val draftSource: String,
    )

    private val SENTENCE_DELIMITERS = setOf('.', '!', '?', '。', '！', '？')
}

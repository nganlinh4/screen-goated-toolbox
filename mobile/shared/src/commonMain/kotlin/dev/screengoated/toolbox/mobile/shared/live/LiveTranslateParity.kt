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

    fun claimTranslationRequest(state: LiveTextState): Pair<LiveTextState, TranslationRequest?> {
        if (state.fullTranscript.length == state.lastProcessedLen) {
            return state to null
        }

        val chunk = getTranslationChunk(state) ?: return state to null
        val nextState = state.copy(
            lastProcessedLen = state.fullTranscript.length,
            uncommittedTranslation = "",
            displayTranslation = updateDisplayTranslation(
                committedTranslation = state.committedTranslation,
                uncommittedTranslation = "",
            ),
        )
        return nextState to TranslationRequest(
            chunk = chunk.text,
            hasFinishedDelimiter = chunk.hasFinishedDelimiter,
            bytesToCommit = chunk.bytesToCommit,
            history = state.translationHistory,
        )
    }

    fun appendTranslationDelta(
        state: LiveTextState,
        newText: String,
        nowMs: Long,
    ): LiveTextState {
        if (newText.isEmpty()) {
            return state
        }

        val uncommitted = state.uncommittedTranslation + newText
        return state.copy(
            uncommittedTranslation = uncommitted,
            displayTranslation = updateDisplayTranslation(
                committedTranslation = state.committedTranslation,
                uncommittedTranslation = uncommitted,
            ),
            lastTranslationUpdateAtMs = nowMs,
        )
    }

    fun finalizeTranslation(
        state: LiveTextState,
        bytesToCommit: Int,
    ): LiveTextState {
        val committedState = commitCurrentTranslation(state)
        return advanceCommittedPos(committedState, bytesToCommit)
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

        var splitIndex: Int? = null
        for (index in text.indices) {
            if (SENTENCE_DELIMITERS.contains(text[index])) {
                splitIndex = index + 1
            }
        }

        return if (splitIndex != null) {
            TranslationChunk(
                text = text.substring(0, splitIndex),
                hasFinishedDelimiter = true,
                bytesToCommit = splitIndex,
            )
        } else {
            TranslationChunk(
                text = text,
                hasFinishedDelimiter = false,
                bytesToCommit = 0,
            )
        }
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

        if (state.lastCommittedPos < state.fullTranscript.length) {
            val pendingLen = state.fullTranscript.length - state.lastCommittedPos
            if (pendingLen < MIN_FORCE_COMMIT_CHARS) {
                return false
            }
        }

        val userSilent = nowMs - state.lastTranscriptAppendAtMs > USER_SILENCE_TIMEOUT_MS
        val aiSilent = nowMs - state.lastTranslationUpdateAtMs > AI_SILENCE_TIMEOUT_MS
        val sourceReady = sourceEndsWithSentence(state) || state.lastCommittedPos < state.fullTranscript.length
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
        if (state.lastCommittedPos >= state.fullTranscript.length) {
            return false
        }

        return state.fullTranscript
            .substring(state.lastCommittedPos)
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
            return state.copy(
                uncommittedTranslation = "",
                displayTranslation = updateDisplayTranslation(
                    committedTranslation = state.committedTranslation,
                    uncommittedTranslation = "",
                ),
            )
        }

        val sourceSegment = if (state.lastCommittedPos < state.fullTranscript.length) {
            state.fullTranscript.substring(state.lastCommittedPos).trim()
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
            lastCommittedPos = state.fullTranscript.length,
            translationHistory = addToHistory(
                history = state.translationHistory,
                source = sourceSegment,
                translation = translatedSegment,
            ),
        )
    }

    private fun commitCurrentTranslation(state: LiveTextState): LiveTextState {
        val translatedSegment = state.uncommittedTranslation.trim()
        if (translatedSegment.isEmpty()) {
            return state.copy(
                uncommittedTranslation = "",
                displayTranslation = updateDisplayTranslation(
                    committedTranslation = state.committedTranslation,
                    uncommittedTranslation = "",
                ),
            )
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
        )
    }

    private fun advanceCommittedPos(
        state: LiveTextState,
        amount: Int,
    ): LiveTextState {
        val nextPos = (state.lastCommittedPos + amount)
            .coerceAtLeast(0)
            .coerceAtMost(state.fullTranscript.length)
        return state.copy(lastCommittedPos = nextPos)
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

    private data class TranslationChunk(
        val text: String,
        val hasFinishedDelimiter: Boolean,
        val bytesToCommit: Int,
    )

    private val SENTENCE_DELIMITERS = setOf('.', '!', '?', '。', '！', '？')
}

package dev.screengoated.toolbox.mobile.shared.live

/**
 * Android port of the Windows-canonical offline-ASR (sherpa) streaming commit
 * machine (`src/api/realtime_audio/offline_asr_commit.rs`). It is asserted
 * byte-for-byte against the shared golden fixtures in
 * `parity-fixtures/offline-asr-stream/cases.json`, the same file the Rust side
 * tests, so the two platforms cannot drift. See `.claude/parity/offline-asr-stream.md`.
 *
 * Replaces the previous divergent inline commit glue in LiveSessionRuntimeOfflineAsr
 * (which had an undocumented isEndpoint branch + DRAFT_STALE_MS clause with no
 * Windows equivalent).
 *
 * NOTE on Unicode: the Windows reference slices/measures in UTF-8 bytes; this port
 * measures in Kotlin chars (UTF-16). The two agree for ASCII and BMP text (incl. the
 * common CJK range); a sentence boundary immediately followed by supplementary-plane
 * text is the one untested edge — documented in the parity spec.
 */

private const val DRAFT_COMMIT_BASE_MS = 1200.0
private const val DRAFT_COMMIT_DECAY = 0.5
private const val PUNCT_STALE_COMMIT_MS = 600L

/** Accumulating state for the offline-ASR commit machine (mirrors OfflineAsrCommitState). */
data class OfflineAsrCommitState(
    var committedHistory: String = "",
    var streamCommittedPrefix: String = "",
    var lastDraftText: String = "",
    var lastDraftChangeMs: Long = 0L,
)

object OfflineAsrStreamParity {

    /** ms a draft must be stable before committing, scaled down by word count. */
    fun draftCommitThresholdMs(draft: String): Long {
        // Each CJK char counts as its own semantic word.
        val cjkCount = draft.count { it.code > 0x2E80 }
        val wordCount = draft.split(Regex("\\s+")).count { it.isNotEmpty() } + cjkCount
        if (wordCount == 0) return Long.MAX_VALUE
        val threshold = DRAFT_COMMIT_BASE_MS / (1.0 + wordCount * DRAFT_COMMIT_DECAY)
        return threshold.toLong()
    }

    /** Commit the draft if it has been silent past its threshold. */
    fun checkDraftCommit(draft: String, silenceMs: Long): String? {
        if (draft.isEmpty()) return null
        return if (silenceMs >= draftCommitThresholdMs(draft)) draft.trimEnd() else null
    }

    /** Split at the last interior `.?!` that has alphanumeric text after it. */
    fun splitAtSentenceBoundary(text: String): Pair<String, String>? {
        var lastBoundary: Int? = null // char index just after the boundary char
        for (i in text.indices) {
            val ch = text[i]
            if (ch == '.' || ch == '?' || ch == '!') {
                val rest = text.substring(i + 1).trimStart()
                val first = rest.firstOrNull()
                if (first != null && (first.isLetter() || first.isDigit())) {
                    lastBoundary = i + 1
                }
            }
        }
        return lastBoundary?.let { pos ->
            Pair(text.substring(0, pos).trimEnd(), text.substring(pos).trimStart())
        }
    }

    private fun sanitizeSegment(segment: String): String =
        segment.replace('\n', ' ').replace('\t', ' ')

    /** Join two transcript segments with smart spacing. */
    fun joinTranscriptSegments(left: String, right: String): String {
        val l = sanitizeSegment(left)
        val r = sanitizeSegment(right)
        return when {
            l.isEmpty() && r.isEmpty() -> ""
            l.isEmpty() -> r.trimStart()
            r.isEmpty() -> l
            else -> {
                val leftHasSpace = l.lastOrNull()?.isWhitespace() == true
                val rightHasSpace = r.firstOrNull()?.isWhitespace() == true
                if (leftHasSpace || rightHasSpace) "$l$r" else "$l $r"
            }
        }
    }

    /** Append a finished segment to history, joining with smart spacing. */
    fun appendHistorySegment(history: String, segment: String): String {
        val seg = sanitizeSegment(segment)
        if (seg.isEmpty()) return history
        return if (history.isEmpty()) seg.trimStart() else joinTranscriptSegments(history, seg)
    }

    /**
     * Advance the commit machine with the latest (already JSON-parsed, trimmed)
     * recognizer text. `nowMs` is a monotonic millisecond clock. Mutates [state] in
     * place and returns the active (uncommitted) draft to render after the history.
     */
    fun commitStep(
        state: OfflineAsrCommitState,
        recognizerText: String,
        hasNativePunctuation: Boolean,
        nowMs: Long,
    ): String {
        val text = recognizerText.trim()
        val draft = if (text.startsWith(state.streamCommittedPrefix)) {
            text.substring(state.streamCommittedPrefix.length).trimStart()
        } else {
            text
        }

        if (draft != state.lastDraftText) {
            state.lastDraftText = draft
            state.lastDraftChangeMs = nowMs
        }
        val elapsedMs = (nowMs - state.lastDraftChangeMs).coerceAtLeast(0L)

        if (hasNativePunctuation) {
            val split = splitAtSentenceBoundary(draft)
            if (split != null) {
                val (before, after) = split
                state.committedHistory = appendHistorySegment(state.committedHistory, before)
                state.streamCommittedPrefix =
                    text.substring(0, text.length - after.length).trimEnd()
                state.lastDraftText = ""
                state.lastDraftChangeMs = nowMs
                return after.trimStart()
            }
            val trimmed = draft.trimEnd()
            val endsSentence = trimmed.endsWith('.') || trimmed.endsWith('?') || trimmed.endsWith('!')
            if (endsSentence && elapsedMs >= PUNCT_STALE_COMMIT_MS) {
                state.committedHistory = appendHistorySegment(state.committedHistory, draft)
                state.streamCommittedPrefix = text.trimEnd()
                state.lastDraftText = ""
                state.lastDraftChangeMs = nowMs
                return ""
            }
            return draft
        }

        val committed = checkDraftCommit(draft, elapsedMs)
        if (committed != null) {
            state.committedHistory = appendHistorySegment(state.committedHistory, "$committed.")
            state.streamCommittedPrefix = text.trimEnd()
            state.lastDraftText = ""
            state.lastDraftChangeMs = nowMs
            return ""
        }
        return draft
    }
}

package dev.screengoated.toolbox.mobile.phonecontrol.runtime

internal data class PhoneControlInputTranscriptUpdate(
    val text: String,
    val startsTurn: Boolean,
    val changed: Boolean,
    val fragmentIndex: Int,
)

/**
 * Joins Live transcription fragments without guessing turn meaning from text.
 * Only a structural signal opens a fresh epoch; late revisions stay attached to
 * the preceding epoch and therefore cannot manufacture another user turn.
 */
internal class PhoneControlInputTranscriptAssembler {
    private val transcript = PhoneControlTranscriptAccumulator()
    private var open = false
    private var freshEpoch = false
    private var fragmentCount = 0

    val text: String
        get() = transcript.text

    val hasFreshEpoch: Boolean
        get() = freshEpoch

    val isOpen: Boolean
        get() = open

    fun merge(fragment: String): PhoneControlInputTranscriptUpdate? {
        val normalized = fragment.trim()
        if (normalized.isEmpty()) return null
        val startsTurn = freshEpoch || !open
        if (startsTurn) {
            transcript.reset(normalized)
            open = true
            freshEpoch = false
            fragmentCount = 1
            return PhoneControlInputTranscriptUpdate(
                text = transcript.text,
                startsTurn = true,
                changed = true,
                fragmentIndex = fragmentCount,
            )
        }
        fragmentCount = if (fragmentCount == Int.MAX_VALUE) Int.MAX_VALUE else fragmentCount + 1
        val changed = transcript.merge(normalized)
        return PhoneControlInputTranscriptUpdate(
            text = transcript.text,
            startsTurn = false,
            changed = changed,
            fragmentIndex = fragmentCount,
        )
    }

    fun beginEpoch() {
        freshEpoch = true
    }

    fun claimCurrentEpoch() {
        if (!open || freshEpoch) {
            transcript.reset()
            open = true
            freshEpoch = false
            fragmentCount = 0
        }
    }
}

internal class PhoneControlTranscriptAccumulator {
    var text: String = ""
        private set

    fun reset(initial: String = "") {
        text = initial.trim()
    }

    fun merge(fragment: String): Boolean {
        val normalized = fragment.trim()
        if (normalized.isEmpty()) return false
        val merged = mergePhoneControlTranscriptText(text, normalized)
        if (merged == text) return false
        text = merged
        return true
    }
}

internal fun mergePhoneControlTranscriptText(existing: String, incoming: String): String {
    val current = existing.trim()
    if (current.isEmpty() || incoming.startsWith(current)) return incoming
    if (current.startsWith(incoming) || current.endsWith(incoming)) return current
    val overlap = incoming.codePointPrefixEnds()
        .filter { length ->
            length <= current.length && current.endsWith(incoming.substring(0, length))
        }
        .maxOrNull()
        ?: 0
    val meaningfulOverlap = overlap > 0 && (
        incoming.substring(0, overlap).any(Char::isWhitespace) ||
            incoming.substring(0, overlap).codePointCount() >= MIN_TRANSCRIPT_OVERLAP_CHARS
        )
    if (meaningfulOverlap) return current + incoming.substring(overlap)
    return current + " " + incoming
}

private fun String.codePointCount(): Int = codePointCount(0, length)

private fun String.codePointPrefixEnds(): Sequence<Int> = sequence {
    var end = 0
    while (end < length) {
        end += Character.charCount(codePointAt(end))
        yield(end)
    }
}

private const val MIN_TRANSCRIPT_OVERLAP_CHARS = 3

package dev.screengoated.toolbox.mobile.shared.live

internal fun sanitizeTranscriptSegment(segment: String): String =
    segment.replace('\n', ' ').replace('\t', ' ')

internal fun translationRequestMatches(state: LiveTextState, request: TranslationRequest): Boolean =
    request.revision == state.translationRequestRevision &&
        request.sourceStart == state.lastCommittedPos && request.sourceStart >= 0 &&
        request.sourceEnd in request.sourceStart..state.fullTranscript.length &&
        state.fullTranscript.substring(request.sourceStart, request.sourceEnd) == request.pendingSource &&
        (state.transcriptionMethod != TranscriptionMethod.GEMINI_TRANSCRIBE ||
            state.transcriptCommittedPos.coerceAtMost(request.sourceEnd) <= request.finalizedSourceEnd)

internal fun processedLengthAfterCorrection(state: LiveTextState, sharedPrefix: Int, committedPos: Int): Int =
    if ((sharedPrefix < state.fullTranscript.length && sharedPrefix < state.lastProcessedLen) ||
        (state.transcriptionMethod == TranscriptionMethod.GEMINI_TRANSCRIBE &&
            committedPos > state.transcriptCommittedPos && state.lastProcessedLen > state.transcriptCommittedPos)) 0 else state.lastProcessedLen

internal fun scalarAlignedSharedPrefix(left: String, right: String): Int {
    val prefix = left.commonPrefixWith(right).length
    return if (prefix > 0 && prefix < left.length && left[prefix - 1] in '\uD800'..'\uDBFF' && left[prefix] in '\uDC00'..'\uDFFF') prefix - 1 else prefix
}

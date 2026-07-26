package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlServiceState
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimePhase
import java.text.BreakIterator

internal data class PhoneControlOverlayVisual(
    val stateLabel: String,
    val iconOverride: String?,
    val caption: String,
    val listeningLevel: Float,
    val visible: Boolean,
)

internal fun phoneControlOverlayVisual(
    state: PhoneControlServiceState,
): PhoneControlOverlayVisual {
    if (!state.running || state.phase in HIDDEN_PHASES) {
        return PhoneControlOverlayVisual(
            GeneratedPhoneControlContract.ORB_STATE_IDLE,
            null,
            "",
            0f,
            false,
        )
    }
    val stateLabel = when (state.phase) {
        PhoneControlRuntimePhase.STARTING,
        PhoneControlRuntimePhase.CONNECTING,
        PhoneControlRuntimePhase.RECONNECTING,
        -> GeneratedPhoneControlContract.ORB_STATE_THINKING
        PhoneControlRuntimePhase.LISTENING -> GeneratedPhoneControlContract.ORB_STATE_IDLE
        PhoneControlRuntimePhase.WORKING,
        PhoneControlRuntimePhase.FINALIZING,
        -> state.orbStateLabel
        PhoneControlRuntimePhase.DEGRADED -> state.orbStateLabel
        PhoneControlRuntimePhase.ERROR,
        PhoneControlRuntimePhase.STOPPED,
        -> GeneratedPhoneControlContract.ORB_STATE_IDLE
    }
    val caption = when (state.phase) {
        PhoneControlRuntimePhase.LISTENING ->
            compactPhoneControlGuidance(state.authorityGuidance)
        PhoneControlRuntimePhase.WORKING -> state.outputCaption.ifBlank { state.inputCaption }
        PhoneControlRuntimePhase.FINALIZING -> state.outputCaption
        PhoneControlRuntimePhase.STARTING,
        PhoneControlRuntimePhase.CONNECTING,
        PhoneControlRuntimePhase.RECONNECTING,
        -> state.userMessage
        PhoneControlRuntimePhase.DEGRADED ->
            compactPhoneControlGuidance(state.authorityGuidance)
        PhoneControlRuntimePhase.ERROR,
        PhoneControlRuntimePhase.STOPPED,
        -> ""
    }
    return PhoneControlOverlayVisual(
        stateLabel = stateLabel,
        iconOverride = state.orbIconOverride,
        caption = caption,
        listeningLevel = state.listeningLevel.coerceIn(0f, 1f),
        visible = true,
    )
}

internal fun compactPhoneControlGuidance(guidance: String): String {
    val normalized = guidance.trim().replace(WHITESPACE, " ")
    if (normalized.codePointCount(0, normalized.length) <= MAX_GUIDANCE_CODE_POINTS) {
        return normalized
    }
    val firstSentenceEnd = BreakIterator.getSentenceInstance().run {
        setText(normalized)
        next()
    }
    if (firstSentenceEnd in 1 until normalized.length) {
        val firstSentence = normalized.substring(0, firstSentenceEnd).trim()
        if (firstSentence.codePointCount(0, firstSentence.length) <= MAX_GUIDANCE_CODE_POINTS) {
            return firstSentence
        }
    }
    val end = normalized.offsetByCodePoints(0, MAX_GUIDANCE_CODE_POINTS - 1)
    val clipped = normalized.substring(0, end).trimEnd()
    val wordBoundary = clipped.lastIndexOf(' ')
        .takeIf { it >= MIN_GUIDANCE_WORD_BOUNDARY }
        ?: clipped.length
    return clipped.substring(0, wordBoundary).trimEnd() + "…"
}

private val HIDDEN_PHASES = setOf(
    PhoneControlRuntimePhase.ERROR,
    PhoneControlRuntimePhase.STOPPED,
)

private val WHITESPACE = Regex("\\s+")
private const val MAX_GUIDANCE_CODE_POINTS = 64
private const val MIN_GUIDANCE_WORD_BOUNDARY = 32

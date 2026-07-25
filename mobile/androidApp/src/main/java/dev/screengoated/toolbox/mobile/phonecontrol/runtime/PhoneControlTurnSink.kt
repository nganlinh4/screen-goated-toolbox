package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase

internal interface PhoneControlTurnSink {
    fun sendPayload(payload: String): Boolean
    fun sendScreenEvidence(payload: String): Boolean = sendPayload(payload)
    fun playAudio(bytes: ByteArray)
    fun interruptPlayback()
    fun discardQueuedPlayback()
    fun updateInputCaption(text: String)
    fun updateOutputCaption(text: String)
    fun surfaceAssistantContent(): Boolean = true
    fun updateOrbPresentation(stateLabel: String, iconOverride: String?) = Unit
    fun updateTurnPhase(phase: PhoneControlTurnPhase)
    fun requestScreenRefresh()
    fun abortProtocolSession() = Unit
}

internal fun PhoneControlTurnSink.updateConversationInputCaption(text: String) {
    if (surfaceAssistantContent()) updateInputCaption(text)
}

internal fun PhoneControlTurnSink.updateConversationOutputCaption(text: String) {
    if (surfaceAssistantContent()) updateOutputCaption(text)
}

internal fun PhoneControlTurnSink.updateConversationOrb(
    stateLabel: String,
    iconOverride: String?,
) {
    if (surfaceAssistantContent()) updateOrbPresentation(stateLabel, iconOverride)
}

internal fun PhoneControlTurnSink.updateConversationPhase(phase: PhoneControlTurnPhase) {
    if (surfaceAssistantContent()) updateTurnPhase(phase)
}

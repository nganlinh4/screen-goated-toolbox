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
    fun updateOrbPresentation(stateLabel: String, iconOverride: String?) = Unit
    fun updateTurnPhase(phase: PhoneControlTurnPhase)
    fun reconciliationRequired()
    fun requestScreenRefresh()
    fun abortProtocolSession() = Unit
}

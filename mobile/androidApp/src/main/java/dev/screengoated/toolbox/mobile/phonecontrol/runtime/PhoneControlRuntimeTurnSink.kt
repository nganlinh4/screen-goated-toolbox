package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase

internal class PhoneControlRuntimeTurnSink(
    private val send: (String) -> Boolean,
    private val sendEvidence: (String) -> Boolean,
    private val play: (ByteArray) -> Unit,
    private val interrupt: () -> Unit,
    private val discard: () -> Unit,
    private val inputCaption: (String) -> Unit,
    private val outputCaption: (String) -> Unit,
    private val orbPresentation: (String, String?) -> Unit,
    private val phase: (PhoneControlTurnPhase) -> Unit,
    private val reconcile: () -> Unit,
    private val refresh: () -> Unit,
    private val abortProtocol: () -> Unit,
) : PhoneControlTurnSink {
    override fun sendPayload(payload: String): Boolean = send(payload)

    override fun sendScreenEvidence(payload: String): Boolean = sendEvidence(payload)

    override fun playAudio(bytes: ByteArray) = play(bytes)

    override fun interruptPlayback() = interrupt()

    override fun discardQueuedPlayback() = discard()

    override fun updateInputCaption(text: String) = inputCaption(text)

    override fun updateOutputCaption(text: String) = outputCaption(text)

    override fun updateOrbPresentation(stateLabel: String, iconOverride: String?) =
        orbPresentation(stateLabel, iconOverride)

    override fun updateTurnPhase(phase: PhoneControlTurnPhase) = this.phase(phase)

    override fun reconciliationRequired() = reconcile()

    override fun requestScreenRefresh() = refresh()

    override fun abortProtocolSession() = abortProtocol()
}

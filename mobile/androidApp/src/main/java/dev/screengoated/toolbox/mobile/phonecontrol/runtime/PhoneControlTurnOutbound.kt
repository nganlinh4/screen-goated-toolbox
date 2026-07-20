package dev.screengoated.toolbox.mobile.phonecontrol.runtime

internal class PhoneControlTurnOutbound(
    private val sink: PhoneControlTurnSink,
) {
    var refused: Boolean = false
        private set

    fun sendPayload(payload: String): Boolean = send { sink.sendPayload(payload) }

    fun sendEvidence(payload: String): Boolean = send { sink.sendScreenEvidence(payload) }

    fun refuse() {
        if (refused) return
        refused = true
        sink.abortProtocolSession()
    }

    fun block() {
        refused = true
    }

    fun reset() {
        refused = false
    }

    private inline fun send(send: () -> Boolean): Boolean {
        if (refused) return false
        if (send()) return true
        refuse()
        return false
    }
}

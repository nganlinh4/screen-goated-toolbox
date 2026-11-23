package dev.screengoated.toolbox.mobile.service

/** Tracks whether captured speech still belongs to the visible playback session. */
internal class S2sVisibilityGate {
    private val lock = Any()
    private var visible = true
    private var active = false
    private var epoch = 0L
    private var session = 0L

    fun start(): Long = synchronized(lock) { epoch++; session++; active = true; session }
    fun stop(id: Long): Boolean = synchronized(lock) {
        if (!active || session != id) return@synchronized false
        epoch++
        active = false
        true
    }
    fun stopActive(): Boolean = synchronized(lock) {
        if (!active) return@synchronized false
        epoch++
        active = false
        true
    }
    fun setVisible(value: Boolean): Boolean = synchronized(lock) {
        if (visible == value) return@synchronized false
        visible = value
        epoch++
        !value
    }
    fun captureGeneration(): Long = synchronized(lock) { epoch }
    fun snapshot(): Long? = synchronized(lock) { if (visible && active) epoch else null }
    fun isCurrent(value: Long?): Boolean = synchronized(lock) {
        value != null && active && visible && value == epoch
    }
    fun dispatch(value: Long?, effect: () -> Unit): Boolean = synchronized(lock) {
        if (value == null || !active || !visible || value != epoch) return@synchronized false
        effect()
        true
    }
}

/** Keeps a previous owner's stop from flushing a newly admitted session. */
internal class S2sVisibilityController(private val stopPlayers: () -> Unit) {
    val gate = S2sVisibilityGate()
    private val transitionLock = Any()

    fun setVisible(visible: Boolean) = synchronized(transitionLock) {
        if (gate.setVisible(visible)) stopPlayers()
    }
    fun start(): Long = synchronized(transitionLock) { gate.start() }
    fun stopActive() = synchronized(transitionLock) { if (gate.stopActive()) stopPlayers() }
    fun stop(id: Long) = synchronized(transitionLock) { if (gate.stop(id)) stopPlayers() }
}

internal data class S2sCapturedAudio(val samples: ShortArray, val epoch: Long?)

internal enum class S2sStreamInput { DROP, KEEP, RESTART }

internal class S2sStreamEpoch(private var epoch: Long?) {
    fun observe(captured: Long?, gate: S2sVisibilityGate): S2sStreamInput {
        if (captured != null && !gate.isCurrent(captured)) return S2sStreamInput.DROP
        if (captured == null && gate.snapshot() != null) return S2sStreamInput.DROP
        if (captured != null && captured != epoch) {
            epoch = captured
            return S2sStreamInput.RESTART
        }
        return S2sStreamInput.KEEP
    }

    fun current(): Long? = epoch
}

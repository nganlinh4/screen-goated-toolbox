package dev.screengoated.toolbox.mobile.phonecontrol.runtime

/** Audio callbacks publish intent; only the coordinator assembles transcripts. */
internal class PhoneControlInputEpochGate {
    private var initial = true
    private var completedTurn: Long? = null
    private var observedCompletedTurn: Long? = null
    private var pendingEpoch = false

    @Synchronized
    fun observe(assistantPlaying: Boolean, burstStarted: Boolean): Boolean {
        val completed = completedTurn
        val beginEpoch = if (completed != null) {
            val crossedBoundary = observedCompletedTurn != completed
            observedCompletedTurn = completed
            burstStarted || crossedBoundary
        } else {
            initial && burstStarted && !assistantPlaying
        }
        if (beginEpoch) pendingEpoch = true
        return beginEpoch || (burstStarted && !assistantPlaying)
    }

    @Synchronized
    fun takePendingEpoch(): Boolean = pendingEpoch.also { pendingEpoch = false }

    @Synchronized
    fun collecting() {
        initial = false
        completedTurn = null
        pendingEpoch = false
    }

    @Synchronized
    fun completed(turn: Long) {
        initial = false
        completedTurn = turn
    }
}

package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import java.util.ArrayDeque

internal data class PhoneControlOutboundRecord(
    val kind: PhoneControlOutboundKind,
    val utf8Bytes: Int,
    val pendingWork: Int,
    val turnPhase: PhoneControlTurnPhase,
    val accepted: Boolean,
    val elapsedMs: Long,
)

/** Retains only bounded protocol shape. Payload content never enters this object. */
internal class PhoneControlOutboundDiagnostics(
    private val clockMs: () -> Long,
) {
    private val lock = Any()
    private val tail = ArrayDeque<PhoneControlOutboundRecord>(MAXIMUM_RECORDS)

    fun record(
        kind: PhoneControlOutboundKind,
        utf8Bytes: Int,
        pendingWork: Int,
        turnPhase: PhoneControlTurnPhase,
        accepted: Boolean,
    ) = synchronized(lock) {
        if (tail.size == MAXIMUM_RECORDS) tail.removeFirst()
        tail.addLast(
            PhoneControlOutboundRecord(
                kind = kind,
                utf8Bytes = utf8Bytes.coerceAtLeast(0),
                pendingWork = pendingWork.coerceAtLeast(0),
                turnPhase = turnPhase,
                accepted = accepted,
                elapsedMs = clockMs(),
            ),
        )
    }

    fun describe(): String = synchronized(lock) {
        val now = clockMs()
        if (tail.isEmpty()) return@synchronized "none"
        tail.joinToString(separator = ",") { record ->
            "${record.kind.contractValue}:${record.utf8Bytes}:" +
                "pending=${record.pendingWork}:phase=${record.turnPhase.name.lowercase()}:" +
                "accepted=${if (record.accepted) 1 else 0}:" +
                "age_ms=${(now - record.elapsedMs).coerceAtLeast(0)}"
        }
    }

    internal companion object {
        const val MAXIMUM_RECORDS = 6
    }
}

internal fun canSendAmbientScreen(pendingWorkCount: Int): Boolean = pendingWorkCount == 0

package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import android.os.SystemClock
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReadySession
import java.nio.charset.StandardCharsets

internal class PhoneControlOutboundSender(
    clockMs: () -> Long = SystemClock::elapsedRealtime,
) {
    private val diagnostics = PhoneControlOutboundDiagnostics(clockMs)

    fun send(
        session: GeminiLiveReadySession,
        payload: String,
        kind: PhoneControlOutboundKind,
        pendingWork: Int,
        turnPhase: PhoneControlTurnPhase,
        utf8Bytes: Int = payload.toByteArray(StandardCharsets.UTF_8).size,
    ): Boolean {
        val accepted = session.trySend(payload)
        diagnostics.record(
            kind = kind,
            utf8Bytes = utf8Bytes,
            pendingWork = pendingWork,
            turnPhase = turnPhase,
            accepted = accepted,
        )
        if (!accepted) {
            Log.w(
                TAG,
                "transport_send_rejected kind=${kind.contractValue} bytes=$utf8Bytes " +
                    "pending=$pendingWork phase=${turnPhase.name.lowercase()}",
            )
        }
        return accepted
    }

    fun describe(): String = diagnostics.describe()

    private companion object {
        const val TAG = "SGTPhoneControl"
    }
}

package dev.screengoated.toolbox.mobile.creation.worker

import dev.screengoated.toolbox.mobile.creation.CreationWorkerEvent

internal class CreationWorkerTerminalRelay(
    private val forward: (CreationWorkerEvent) -> Unit,
) {
    private var terminal: CreationWorkerEvent? = null

    fun accept(event: CreationWorkerEvent) {
        if (event.event in TERMINAL_EVENTS) {
            if (terminal == null) terminal = event
        } else if (terminal == null) {
            forward(event)
        }
    }

    fun complete(fallback: CreationWorkerEvent): CreationWorkerEvent = terminal ?: fallback

    private companion object {
        val TERMINAL_EVENTS = setOf("success", "failure", "cancelled")
    }
}

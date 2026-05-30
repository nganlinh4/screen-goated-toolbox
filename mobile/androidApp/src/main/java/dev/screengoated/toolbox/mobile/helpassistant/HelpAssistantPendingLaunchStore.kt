package dev.screengoated.toolbox.mobile.helpassistant

import java.util.concurrent.atomic.AtomicReference

data class PendingHelpAssistantLaunch(
    val question: String,
    val uiLanguage: String,
)

object HelpAssistantPendingLaunchStore {
    private val pending = AtomicReference<PendingHelpAssistantLaunch?>(null)

    fun set(question: String, uiLanguage: String) {
        val trimmed = question.trim()
        if (trimmed.isBlank()) {
            return
        }
        pending.set(PendingHelpAssistantLaunch(question = trimmed, uiLanguage = uiLanguage))
    }

    fun take(): PendingHelpAssistantLaunch? = pending.getAndSet(null)
}

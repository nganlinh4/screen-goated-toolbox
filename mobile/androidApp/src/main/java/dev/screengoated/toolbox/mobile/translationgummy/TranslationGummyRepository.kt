package dev.screengoated.toolbox.mobile.translationgummy

import dev.screengoated.toolbox.mobile.service.tts.DeviceLanguageDetector
import dev.screengoated.toolbox.mobile.storage.SecureSettingsStore
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.util.concurrent.atomic.AtomicLong

class TranslationGummyRepository(
    private val settingsStore: SecureSettingsStore,
    private val languageDetector: DeviceLanguageDetector,
) {
    private val savedConfig = settingsStore.loadTranslationGummyConfig().normalized()
    private val savedTranscripts = settingsStore.loadTranslationGummyTranscripts()
    private val transcriptIdCounter = AtomicLong(
        (savedTranscripts.maxOfOrNull { it.id } ?: 0L) + 1L,
    )
    private val mutableState = MutableStateFlow(
        TranslationGummyState(
            appliedConfig = savedConfig,
            draftConfig = savedConfig,
            guideSeen = savedConfig.guideSeen,
            transcripts = savedTranscripts,
        ).normalize(),
    )

    val state: StateFlow<TranslationGummyState> = mutableState.asStateFlow()

    fun updateDraft(
        transform: (TranslationGummyConfig) -> TranslationGummyConfig,
    ) {
        // Don't call .normalized() here — trim() strips trailing spaces while typing.
        // Normalization happens in applyDraft() and buildSystemInstruction() instead.
        mutableState.value = mutableState.value.copy(
            draftConfig = transform(mutableState.value.draftConfig),
        ).normalize()
    }

    fun applyDraft(): TranslationGummyConfig {
        val applied = mutableState.value.draftConfig.normalized()
            .copy(guideSeen = mutableState.value.guideSeen)
        settingsStore.saveTranslationGummyConfig(applied)
        mutableState.value = mutableState.value.copy(
            appliedConfig = applied,
            draftConfig = applied,
            guideSeen = applied.guideSeen,
            dirty = false,
            lastError = null,
            transcripts = emptyList(),
        ).normalize()
        return applied
    }

    fun currentAppliedConfig(): TranslationGummyConfig = mutableState.value.appliedConfig.normalized()

    fun dismissGuide() {
        val applied = mutableState.value.appliedConfig.normalized().copy(guideSeen = true)
        settingsStore.saveTranslationGummyConfig(applied)
        mutableState.value = mutableState.value.copy(
            appliedConfig = applied,
            draftConfig = mutableState.value.draftConfig.copy(guideSeen = true),
            guideSeen = true,
        ).normalize()
    }

    fun currentApiKey(): String = settingsStore.loadApiKey().trim()

    fun currentGeminiVoice(): String = settingsStore.loadGlobalTtsSettings().voice.trim()

    fun currentGeminiModel(): String = settingsStore.loadGlobalTtsSettings().geminiModel.trim()

    fun localeText(): MobileLocaleText =
        MobileLocaleText.forLanguage(settingsStore.loadUiPreferences().uiLanguage)

    fun markNotConfigured() {
        mutableState.value = mutableState.value.copy(
            connectionState = TranslationGummyConnectionState.NOT_CONFIGURED,
            isRunning = false,
            lastError = null,
            visualizerLevel = 0f,
        ).normalize()
    }

    fun markConnecting(reconnecting: Boolean) {
        mutableState.value = mutableState.value.copy(
            connectionState = if (reconnecting) {
                TranslationGummyConnectionState.RECONNECTING
            } else {
                TranslationGummyConnectionState.CONNECTING
            },
            isRunning = true,
            lastError = null,
        ).normalize()
    }

    fun markReady() {
        insertSessionSeparator()
        mutableState.value = mutableState.value.copy(
            connectionState = TranslationGummyConnectionState.READY,
            isRunning = true,
            lastError = null,
        ).normalize()
    }

    fun insertSessionSeparator() {
        val transcripts = mutableState.value.transcripts
        if (transcripts.isEmpty()) return
        if (transcripts.last().role == TranslationGummyTranscriptRole.SEPARATOR) return
        val formatter = java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault())
        val timeText = formatter.format(java.util.Date())
        val updated = transcripts + TranslationGummyTranscriptItem(
            id = transcriptIdCounter.getAndIncrement(),
            role = TranslationGummyTranscriptRole.SEPARATOR,
            text = timeText,
            isFinal = true,
            updatedAtMs = android.os.SystemClock.elapsedRealtime(),
            lang = "",
        )
        mutableState.value = mutableState.value.copy(
            transcripts = updated.takeLast(MAX_TRANSCRIPTS),
        ).normalize()
        persistTranscripts()
    }

    fun markStopped() {
        mutableState.value = mutableState.value.copy(
            connectionState = if (mutableState.value.appliedConfig.isValid()) {
                TranslationGummyConnectionState.STOPPED
            } else {
                TranslationGummyConnectionState.NOT_CONFIGURED
            },
            isRunning = false,
            visualizerLevel = 0f,
        ).normalize()
    }

    fun fail(message: String) {
        mutableState.value = mutableState.value.copy(
            connectionState = TranslationGummyConnectionState.ERROR,
            isRunning = false,
            lastError = message,
            visualizerLevel = 0f,
        ).normalize()
    }

    fun clearError() {
        mutableState.value = mutableState.value.copy(lastError = null).normalize()
    }

    fun clearTranscripts() {
        mutableState.value = mutableState.value.copy(transcripts = emptyList()).normalize()
        persistTranscripts()
    }

    fun updateVisualizerLevel(level: Float) {
        mutableState.value = mutableState.value.copy(
            visualizerLevel = level.coerceIn(0f, 1f),
        ).normalize()
    }

    fun upsertTranscript(
        role: TranslationGummyTranscriptRole,
        text: String,
        final: Boolean,
        nowMs: Long,
    ) {
        val trimmed = text.trim()
        if (trimmed.isEmpty()) {
            return
        }
        val existing = mutableState.value.transcripts
        val idx = existing.indexOfLast { it.role == role && !it.isFinal }
        val updated = existing.toMutableList()
        if (idx >= 0) {
            // Merge into unfinal item of same role
            val merged = mergeTranscriptText(updated[idx].text, trimmed)
            val lang = detectTranscriptLang(role, merged, updated[idx].lang, final)
            updated[idx] = updated[idx].copy(
                text = merged,
                isFinal = final,
                updatedAtMs = nowMs,
                lang = lang,
            )
        } else if (updated.isNotEmpty() && updated.last().role == role && updated.last().isFinal) {
            // Merge late fragment into the last finalized item of same role
            // (Gemini splits long translations into multiple chunks after turnComplete)
            val lastIdx = updated.lastIndex
            val merged = mergeTranscriptText(updated[lastIdx].text, trimmed)
            val lang = detectTranscriptLang(role, merged, updated[lastIdx].lang, final = true)
            updated[lastIdx] = updated[lastIdx].copy(
                text = merged,
                updatedAtMs = nowMs,
                lang = lang,
            )
        } else {
            updated += TranslationGummyTranscriptItem(
                id = transcriptIdCounter.getAndIncrement(),
                role = role,
                text = trimmed,
                isFinal = final,
                updatedAtMs = nowMs,
                lang = detectTranscriptLang(role, trimmed, existingLang = "", final = final),
            )
        }
        mutableState.value = mutableState.value.copy(
            transcripts = updated.takeLast(MAX_TRANSCRIPTS),
        ).normalize()
    }

    fun finalizeActiveTranscripts(nowMs: Long) {
        val updated = mutableState.value.transcripts.map { item ->
            if (item.isFinal) item else item.copy(isFinal = true, updatedAtMs = nowMs)
        }
        mutableState.value = mutableState.value.copy(transcripts = updated).normalize()
        persistTranscripts()
    }

    private fun TranslationGummyState.normalize(): TranslationGummyState {
        val applied = appliedConfig.normalized()
        // Compare normalized versions for dirty check, but keep raw draft for typing
        val draftNormalized = draftConfig.normalized()
        val connection = when {
            !applied.isValid() && !isRunning -> TranslationGummyConnectionState.NOT_CONFIGURED
            else -> connectionState
        }
        return copy(
            appliedConfig = applied,
            dirty = draftNormalized != applied,
            connectionState = connection,
            guideSeen = guideSeen || applied.guideSeen || draftNormalized.guideSeen,
        )
    }

    private fun detectTranscriptLang(
        role: TranslationGummyTranscriptRole,
        text: String,
        existingLang: String,
        final: Boolean,
    ): String {
        if (role != TranslationGummyTranscriptRole.OUTPUT) {
            return ""
        }
        return if (existingLang.isBlank() || final) {
            languageDetector.detectIso639_3(text).ifBlank { existingLang }
        } else {
            existingLang
        }
    }

    private fun persistTranscripts() {
        settingsStore.saveTranslationGummyTranscripts(
            mutableState.value.transcripts.filter { it.isFinal },
        )
    }

    private companion object {
        private const val MAX_TRANSCRIPTS = 200

        private fun mergeTranscriptText(existing: String, incoming: String): String {
            val current = existing.trim()
            val next = incoming.trim()
            if (current.isEmpty()) return next
            if (next.isEmpty()) return current
            if (next.startsWith(current) || next.contains(current)) return next
            if (current.startsWith(next) || current.contains(next) || current.endsWith(next)) return current
            if (current.endsWith(" ") || next.startsWith(" ") || next.first() in ",.!?:;)]}") {
                return current + next
            }
            return "$current $next"
        }
    }
}

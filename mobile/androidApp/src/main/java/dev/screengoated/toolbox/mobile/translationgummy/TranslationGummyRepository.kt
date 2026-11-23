package dev.screengoated.toolbox.mobile.translationgummy

import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.service.tts.DeviceLanguageDetector
import dev.screengoated.toolbox.mobile.storage.SecureSettingsStore
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.Executors

class TranslationGummyRepository(
    private val settingsStore: SecureSettingsStore,
    private val languageDetector: DeviceLanguageDetector,
) {
    private val savedConfig = settingsStore.loadTranslationGummyConfig().normalized()
    private val savedTranscripts = settingsStore.loadTranslationGummyTranscripts()
    private val transcriptIdCounter = AtomicLong(
        (savedTranscripts.maxOfOrNull { it.id } ?: 0L) + 1L,
    )
    private val transcriptWriter = CoalescingSnapshotWriter(
        executor = Executors.newSingleThreadExecutor { task ->
            Thread(task, "gummy-transcript-persistence").apply { isDaemon = true }
        },
        snapshot = { mutableState.value.transcripts.filter { it.isFinal } },
        save = settingsStore::saveTranslationGummyTranscripts,
        onFailure = { android.util.Log.e("SGTTranslationGummy", "Unable to persist transcripts", it) },
    )
    private val presentation = TranslationGummyPresentation(
        TranslationGummyState(
            appliedConfig = savedConfig,
            draftConfig = savedConfig,
            guideSeen = savedConfig.guideSeen,
            transcripts = savedTranscripts,
        ).normalize(),
    )

    private val mutableState = presentation.mutableState
    val state: StateFlow<TranslationGummyState> = presentation.state
    val visualizerLevels = presentation.visualizerLevels

    fun updateDraft(
        transform: (TranslationGummyConfig) -> TranslationGummyConfig,
    ) {
        // Don't call .normalized() here — trim() strips trailing spaces while typing.
        // Normalization happens in applyDraft() and buildSystemInstruction() instead.
        mutableState.update { current -> current.copy(draftConfig = transform(current.draftConfig)).normalize() }
    }

    fun applyDraft(): TranslationGummyConfig {
        presentation.resetMeter()
        val previousState = mutableState.value
        val applied = previousState.draftConfig.normalized()
            .copy(guideSeen = previousState.guideSeen)
        settingsStore.saveTranslationGummyConfig(applied)
        val separatorId = transcriptIdCounter.getAndIncrement()
        val separatorText = currentSessionSeparatorText()
        val nowMs = android.os.SystemClock.elapsedRealtime()
        mutableState.update { current -> current.afterApplyingDraftForWindowsParity(applied, separatorId, separatorText, nowMs).normalize() }
        persistTranscripts()
        return applied
    }

    fun currentAppliedConfig(): TranslationGummyConfig = mutableState.value.appliedConfig.normalized()

    fun dismissGuide() {
        val applied = mutableState.value.appliedConfig.normalized().copy(guideSeen = true)
        settingsStore.saveTranslationGummyConfig(applied)
        mutableState.update { current -> current.copy(
            appliedConfig = applied,
            draftConfig = current.draftConfig.copy(guideSeen = true),
            guideSeen = true,
        ).normalize() }
    }

    fun currentApiKey(): String = settingsStore.loadApiKey().trim()

    fun currentGeminiVoice(): String = settingsStore.loadGlobalTtsSettings().voice.trim()

    fun currentGeminiModel(): String = RealtimeModelIds.normalizeTtsGeminiModel(
        settingsStore.loadGlobalTtsSettings().geminiModel.trim(),
    )

    fun localeText(): MobileLocaleText =
        MobileLocaleText.forLanguage(settingsStore.loadUiPreferences().uiLanguage)

    fun markNotConfigured() {
        presentation.resetMeter()
        mutableState.update { it.copy(
            connectionState = TranslationGummyConnectionState.NOT_CONFIGURED,
            isRunning = false,
            lastError = null,
        ).normalize() }
    }

    fun markConnecting(reconnecting: Boolean) {
        presentation.resetMeter()
        mutableState.update { it.copy(
            connectionState = if (reconnecting) {
                TranslationGummyConnectionState.RECONNECTING
            } else {
                TranslationGummyConnectionState.CONNECTING
            },
            isRunning = true,
            lastError = null,
        ).normalize() }
    }

    fun markReady() {
        insertSessionSeparator()
        mutableState.update { it.copy(
            connectionState = TranslationGummyConnectionState.READY,
            isRunning = true,
            lastError = null,
        ).normalize() }
    }

    fun insertSessionSeparator() {
        val separator = TranslationGummyTranscriptItem(
            id = transcriptIdCounter.getAndIncrement(),
            role = TranslationGummyTranscriptRole.SEPARATOR,
            text = currentSessionSeparatorText(),
            isFinal = true,
            updatedAtMs = android.os.SystemClock.elapsedRealtime(),
            lang = "",
        )
        mutableState.update { current ->
            val transcripts = current.transcripts
            if (transcripts.isEmpty() || transcripts.last().role == TranslationGummyTranscriptRole.SEPARATOR) current
            else current.copy(transcripts = (transcripts + separator).takeLast(MAX_TRANSCRIPTS)).normalize()
        }
        persistTranscripts()
    }

    fun markStopped() {
        presentation.resetMeter()
        mutableState.update { current -> current.copy(
            connectionState = if (current.appliedConfig.isValid()) {
                TranslationGummyConnectionState.STOPPED
            } else {
                TranslationGummyConnectionState.NOT_CONFIGURED
            },
            isRunning = false,
        ).normalize() }
    }

    fun fail(message: String) {
        presentation.resetMeter()
        mutableState.update { it.copy(
            connectionState = TranslationGummyConnectionState.ERROR,
            isRunning = false,
            lastError = message,
        ).normalize() }
    }

    fun clearError() {
        mutableState.update { it.copy(lastError = null).normalize() }
    }

    fun clearTranscripts() {
        mutableState.update { it.copy(transcripts = emptyList()).normalize() }
        persistTranscripts()
    }

    fun updateVisualizerLevel(level: Float) {
        presentation.updateVisualizerLevel(level)
    }

    fun updateVolumePercent(volumePercent: Int) {
        mutableState.update { it.copy(volume = it.volume.withPercent(volumePercent)).normalize() }
    }

    fun toggleMuted() {
        mutableState.update { it.copy(volume = it.volume.toggleMuted()).normalize() }
    }

    fun currentOutputVolumePercent(): Int = mutableState.value.volume.percent

    internal fun prepareTranscript(
        role: TranslationGummyTranscriptRole,
        text: String,
        final: Boolean,
        nowMs: Long,
    ): PreparedGummyTranscript? {
        val trimmed = text.trim()
        if (trimmed.isEmpty()) return null
        val existing = mutableState.value.transcripts
        val idx = existing.indexOfLast { it.role == role && !it.isFinal }
        val updated = existing.toMutableList()
        val index: Int
        if (idx >= 0) {
            val merged = mergeTranscriptText(updated[idx].text, trimmed)
            updated[idx] = updated[idx].copy(
                text = merged,
                isFinal = final,
                updatedAtMs = nowMs,
            )
            index = idx
        } else if (updated.isNotEmpty() && updated.last().role == role && updated.last().isFinal) {
            val lastIdx = updated.lastIndex
            val merged = mergeTranscriptText(updated[lastIdx].text, trimmed)
            updated[lastIdx] = updated[lastIdx].copy(
                text = merged,
                updatedAtMs = nowMs,
            )
            index = lastIdx
        } else {
            updated += TranslationGummyTranscriptItem(
                id = transcriptIdCounter.getAndIncrement(),
                role = role,
                text = trimmed,
                isFinal = final,
                updatedAtMs = nowMs,
                lang = "",
            )
            index = updated.lastIndex
        }
        return PreparedGummyTranscript(existing, updated, index)
    }

    internal fun commitTranscript(prepared: PreparedGummyTranscript, language: String): Boolean =
        commitPreparedGummyTranscript(mutableState, prepared, language, MAX_TRANSCRIPTS) { it.normalize() }

    fun finalizeActiveTranscripts(nowMs: Long) {
        mutableState.update { current -> current.copy(transcripts = current.transcripts.map { item ->
            if (item.isFinal) item else item.copy(isFinal = true, updatedAtMs = nowMs)
        }).normalize() }
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

    internal fun detectTranscriptLanguage(text: String): String = languageDetector.detectIso639_3(text)
    internal fun fallbackTranscriptLanguage(text: String): String = languageDetector.fallbackIso639_3(text)

    private fun persistTranscripts() = transcriptWriter.request()

    private companion object {
        private const val MAX_TRANSCRIPTS = 200

        private fun currentSessionSeparatorText(): String {
            val formatter = java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault())
            return formatter.format(java.util.Date())
        }

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

internal fun TranslationGummyState.afterApplyingDraftForWindowsParity(
    applied: TranslationGummyConfig,
    nextSeparatorId: Long,
    separatorText: String,
    nowMs: Long,
): TranslationGummyState {
    val shouldInsertSeparator = transcripts.isNotEmpty() &&
        transcripts.last().role != TranslationGummyTranscriptRole.SEPARATOR
    val nextTranscripts = if (shouldInsertSeparator) {
        transcripts + TranslationGummyTranscriptItem(
            id = nextSeparatorId,
            role = TranslationGummyTranscriptRole.SEPARATOR,
            text = separatorText,
            isFinal = true,
            updatedAtMs = nowMs,
            lang = "",
        )
    } else {
        transcripts
    }
    return copy(
        appliedConfig = applied,
        draftConfig = applied,
        guideSeen = applied.guideSeen,
        dirty = false,
        lastError = null,
        transcripts = nextTranscripts.takeLast(200),
    )
}

package dev.screengoated.toolbox.mobile.shared.live

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

class LiveSessionStore(
    initialState: LiveSessionState = LiveSessionState(),
) {
    private val mutableState = MutableStateFlow(initialState)

    val state: StateFlow<LiveSessionState> = mutableState.asStateFlow()

    fun hydrate(config: LiveSessionConfig, permissions: PermissionSnapshot) {
        mutableState.value = LiveSessionState(
            phase = SessionPhase.IDLE,
            config = config,
            permissions = permissions,
            liveText = LiveTranslateParity.reset(),
        )
    }

    fun updateConfig(patch: LiveSessionPatch) {
        mutableState.update { current ->
            current.copy(
                config = current.config.copy(
                    sourceMode = patch.sourceMode ?: current.config.sourceMode,
                    displayMode = patch.displayMode ?: current.config.displayMode,
                    targetLanguage = patch.targetLanguage ?: current.config.targetLanguage,
                    transcriptionProvider = patch.transcriptionProvider ?: current.config.transcriptionProvider,
                    translationProvider = patch.translationProvider ?: current.config.translationProvider,
                    authMode = patch.authMode ?: current.config.authMode,
                    engineKind = patch.engineKind ?: current.config.engineKind,
                    keepOverlayOnTop = patch.keepOverlayOnTop ?: current.config.keepOverlayOnTop,
                    notificationPersistent = patch.notificationPersistent ?: current.config.notificationPersistent,
                ),
            )
        }
    }

    fun markAwaitingPermissions(permissions: PermissionSnapshot) {
        mutableState.update { current ->
            current.copy(
                phase = SessionPhase.AWAITING_PERMISSIONS,
                permissions = permissions,
                lastError = null,
            )
        }
    }

    fun markStarting() {
        mutableState.update { current ->
            current.copy(
                phase = SessionPhase.STARTING,
                liveText = LiveTranslateParity.reset(
                    transcriptionMethod = current.liveText.transcriptionMethod,
                ),
                lastError = null,
            )
        }
    }

    fun markListening() {
        mutableState.update { current ->
            if (current.phase == SessionPhase.LISTENING && current.lastError == null) {
                return@update current
            }
            current.copy(
                phase = SessionPhase.LISTENING,
                lastError = null,
            )
        }
    }

    fun markTranslating() {
        mutableState.update { current ->
            if (current.phase == SessionPhase.TRANSLATING && current.lastError == null) {
                return@update current
            }
            current.copy(
                phase = SessionPhase.TRANSLATING,
                lastError = null,
            )
        }
    }

    fun updatePermissions(permissions: PermissionSnapshot) {
        mutableState.update { current ->
            if (current.permissions == permissions) {
                return@update current
            }
            current.copy(permissions = permissions)
        }
    }

    fun setTranscriptionMethod(method: TranscriptionMethod) {
        mutableState.update { current ->
            current.copy(
                liveText = LiveTranslateParity.setTranscriptionMethod(current.liveText, method),
            )
        }
    }

    fun appendTranscript(
        transcript: String,
        nowMs: Long,
    ) {
        mutableState.update { current ->
            current.copy(
                liveText = LiveTranslateParity.appendTranscript(
                    state = current.liveText,
                    newText = transcript,
                    nowMs = nowMs,
                ),
                lastError = null,
            )
        }
    }

    fun claimTranslationRequest(): TranslationRequest? {
        var request: TranslationRequest? = null
        mutableState.update { current ->
            val (liveText, nextRequest) = LiveTranslateParity.claimTranslationRequest(current.liveText)
            request = nextRequest
            current.copy(
                liveText = liveText,
                lastError = null,
            )
        }
        return request
    }

    fun appendTranslationDelta(
        text: String,
        nowMs: Long,
    ) {
        mutableState.update { current ->
            current.copy(
                liveText = LiveTranslateParity.appendTranslationDelta(
                    state = current.liveText,
                    newText = text,
                    nowMs = nowMs,
                ),
                lastError = null,
            )
        }
    }

    fun finalizeTranslation(bytesToCommit: Int) {
        mutableState.update { current ->
            current.copy(
                liveText = LiveTranslateParity.finalizeTranslation(
                    state = current.liveText,
                    bytesToCommit = bytesToCommit,
                ),
                lastError = null,
            )
        }
    }

    fun forceCommitIfDue(nowMs: Long): Boolean {
        var committed = false
        mutableState.update { current ->
            val (liveText, didCommit) = LiveTranslateParity.forceCommitIfDue(
                state = current.liveText,
                nowMs = nowMs,
            )
            committed = didCommit
            current.copy(liveText = liveText)
        }
        return committed
    }

    fun clearTranslationHistory() {
        mutableState.update { current ->
            current.copy(
                liveText = LiveTranslateParity.clearTranslationHistory(current.liveText),
            )
        }
    }

    fun setOverlayVisible(visible: Boolean) {
        mutableState.update { current ->
            if (current.overlayVisible == visible) {
                return@update current
            }
            current.copy(overlayVisible = visible)
        }
    }

    fun updateMetrics(metrics: LiveSessionMetrics) {
        mutableState.update { current ->
            if (current.metrics == metrics) {
                return@update current
            }
            current.copy(metrics = metrics)
        }
    }

    fun fail(message: String) {
        mutableState.update { current ->
            current.copy(
                phase = SessionPhase.ERROR,
                lastError = message,
            )
        }
    }

    fun stop() {
        mutableState.update { current ->
            current.copy(
                phase = SessionPhase.STOPPED,
                overlayVisible = false,
            )
        }
    }
}

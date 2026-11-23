package dev.screengoated.toolbox.mobile.service.preset

import android.widget.Toast
import dev.screengoated.toolbox.mobile.preset.AudioPresetLaunchKind
import dev.screengoated.toolbox.mobile.preset.audioResumeKind
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.preset.initialAudioBlocks
import dev.screengoated.toolbox.mobile.preset.resolvePrompt
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

// Preset launch + input-submission flow extracted from PresetOverlayController.

internal fun PresetOverlayController.launchPreset(
    presetId: String,
    closePanel: Boolean,
    continuousMode: Boolean,
) {
    cancelPendingAudioRecovery()
    if (closePanel) {
        panelModule.dismiss()
    }
    val stored = presetRepository.getResolvedPreset(presetId) ?: return
    val resolved = if (continuousMode &&
        stored.preset.presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT) {
        stored.copy(preset = stored.preset.copy(continuousInput = true))
    } else stored
    if (continuousMode && continuousSelection.presetId == presetId) {
        selectionCaptureGeneration++
        selectionCapturePending = false
        continuousSelection.close()
        return
    }
    selectionCaptureGeneration++
    selectionCapturePending = false
    continuousSelection.close()
    if (audioCaptureSession.toggleOrAbortIfMatching(presetId)) {
        return
    }
    if (retireUnsupportedPresetLaunch(resolved, presetRepository::cancelExecution)) {
        Toast.makeText(
            context,
            placeholderReasonLabel(
                resolved.executionCapability.reason ?: PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY,
                uiLanguage(),
            ),
            Toast.LENGTH_SHORT,
        ).show()
        return
    }
    if (requiresAccessibilityForAudioAutoPaste(resolved) && !SgtAccessibilityService.isAvailable) {
        promptAccessibilityDisclosure()
        return
    }

    if (imageContinuousPresetId != null && (imageContinuousPresetId != presetId || !continuousMode)) {
        stopImageContinuousMode(showToast = false)
    }

    inputModule.close()
    imageCaptureSession.destroy()
    presetRepository.cancelExecution()
    presetRepository.resetState()
    pendingImageBytes = null
    pendingTextSelectInput = null
    imageContinuousRearmPending = false
    activePreset = resolved

    if (resolved.preset.presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT) {
        // Gate: require accessibility service enabled. Show the prominent
        // disclosure first (Google Play requirement), then open Settings on consent.
        if (!SgtAccessibilityService.isAvailable) {
            promptAccessibilityDisclosure()
            return
        }

        if (continuousMode) {
            continuousSelection.open(resolved)
        } else {
            capturePresetSelection(resolved)
        }
    } else if (resolved.preset.presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE) {
        launchImagePreset(resolved, continuousMode)
    } else if (
        resolved.preset.presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC ||
            resolved.preset.presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO
    ) {
        imageContinuousPresetId = null
        launchAudioPreset(resolved)
    } else {
        imageContinuousPresetId = null
        inputModule.open(resolved)
    }
    if (!closePanel) {
        // Panel doesn't overlap bubble — no z-reorder needed
    }
}

internal fun PresetOverlayController.launchDefaultMicPreset() {
    cancelPendingAudioRecovery()
    val resolved = presetRepository.getResolvedPreset("preset_transcribe") ?: return
    if (!inputModule.hasWindow()) {
        // No input window — normal preset launch
        launchPreset(presetId = resolved.preset.id, closePanel = false, continuousMode = false)
        return
    }
    val inputWindowId = inputModule.currentWindowId() ?: return
    // Input window is open — mic is just a speech-to-text input method.
    // Record audio, transcribe, inject text into input. Do NOT run the preset pipeline.
    // (matches Windows: show_recording_overlay → set_editor_text, input window stays open)
    if (audioCaptureSession.toggleOrAbortIfMatching(resolved.preset.id)) return
    onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.MICROPHONE)
    audioCaptureSession.start(
        resolvedPreset = resolved,
        onRecordingComplete = { capture ->
            onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
            if (inputModule.currentWindowId() != inputWindowId) return@start
            val transcript = capture.precomputedTranscript
            if (!transcript.isNullOrBlank()) {
                // Streaming transcript is already available from the capture session.
                inputModule.injectText(transcript)
                inputModule.bringToFront()
            } else {
                // Standard runtime (Whisper/Groq) — call transcription API directly
                val audioBlock = resolved.preset.initialAudioBlocks().firstOrNull() ?: return@start
                scope.launch(Dispatchers.Main) {
                    val result = appContainer.audioApiClient.executeStreaming(
                        modelId = audioBlock.model,
                        prompt = audioBlock.resolvePrompt(),
                        wavBytes = capture.wavBytes,
                        apiKeys = buildApiKeys(),
                        uiLanguage = uiLanguage(),
                        inputLanguage = audioBlock.languageVars["input_language"],
                        onChunk = {},
                    )
                    if (inputModule.currentWindowId() != inputWindowId) return@launch
                    result.getOrNull()?.takeIf { it.isNotBlank() }?.let { text ->
                        inputModule.injectText(text)
                        inputModule.bringToFront()
                    }
                    result.exceptionOrNull()?.let { error ->
                        apiKeyErrorToastText(error.message ?: error.toString(), uiLanguage())
                            ?.let(appContainer.toastBus::show)
                    }
                }
            }
        },
        onCancelled = {
            onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
        },
        onFailure = { failure ->
            onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
            if (inputModule.currentWindowId() == inputWindowId) {
                handleAudioCaptureFailure(resolved, failure, AudioPresetLaunchKind.INPUT_DICTATION, inputWindowId)
            }
        },
    )
}

internal fun PresetOverlayController.resumePendingAudioLaunch(requestId: String) {
    val pending = appContainer.audioPresetLaunchStore.takeIfId(requestId, audioRecoveryOwnerId) ?: return
    when (audioResumeKind(pending, audioRecoveryOwnerId, inputModule.currentWindowId())) {
        AudioPresetLaunchKind.CAPTURE -> launchPreset(
            presetId = pending.presetId,
            closePanel = false,
            continuousMode = false,
        )
        AudioPresetLaunchKind.INPUT_DICTATION -> launchDefaultMicPreset()
        null -> Unit
    }
}

/**
 * Handle TEXT_SELECT after the selected text has been captured.
 * Fixed prompt → execute immediately.
 * Dynamic prompt → show input window, user types prompt, then execute with modified preset.
 * Matches Windows pipeline.rs:299-358.
 */
internal fun PresetOverlayController.executeTextSelectWithCapturedText(resolved: ResolvedPreset, capturedText: String) {
    if (resolved.preset.promptMode == "dynamic") {
        // Dynamic: show input window for user to type the prompt
        // Store captured text for later — will combine with user's prompt on submit
        pendingTextSelectInput = capturedText
        inputModule.open(resolved)
    } else {
        // Fixed: execute immediately with preset's built-in prompt
        presetRepository.executePreset(resolved.preset, PresetInput.Text(capturedText))
    }
}

internal fun PresetOverlayController.handleInputClosedWithoutResults() {
    val hadPendingImage = pendingImageBytes != null
    pendingImageBytes = null
    pendingTextSelectInput = null
    if (hadPendingImage && imageContinuousPresetId != null) {
        val resolved = activePreset?.takeIf { it.preset.id == imageContinuousPresetId }
            ?: presetRepository.getResolvedPreset(imageContinuousPresetId!!)
        if (resolved != null) {
            startImageCaptureSession(
                resolved = resolved,
                continuousMode = true,
                trace = newImageCaptureTrace(
                    resolved = resolved,
                    continuousMode = true,
                    source = "dynamic_input_cancel_rearm",
                ),
            )
            return
        }
    }
    activePreset = null
}

internal fun PresetOverlayController.submitInput(submission: TextInputSubmission) {
    val text = submission.text
    val resolved = activePreset ?: return
    val pendingImage = pendingImageBytes
    if (pendingImage != null) {
        // IMAGE + dynamic prompt: inject user's prompt, execute with captured image
        pendingImageBytes = null
        val modifiedPreset = mutateDynamicPromptPreset(resolved, text)
        presetRepository.resetState()
        presetRepository.executePreset(modifiedPreset, PresetInput.Image(pendingImage))
        imageContinuousRearmPending = imageContinuousPresetId == resolved.preset.id
        inputModule.recordSubmittedText(submission)
        return
    }
    val pending = pendingTextSelectInput
    if (pending != null) {
        // TEXT_SELECT + dynamic prompt: inject user's prompt into preset, execute with captured text
        // Matches Windows pipeline.rs:323-335
        pendingTextSelectInput = null
        val modifiedPreset = mutateDynamicPromptPreset(resolved, text)
        presetRepository.resetState()
        presetRepository.executePreset(modifiedPreset, PresetInput.Text(pending))
        inputModule.recordSubmittedText(submission)
    } else {
        // Normal TEXT_INPUT flow
        presetRepository.resetState()
        presetRepository.executePreset(resolved.preset, PresetInput.Text(text))
        inputModule.recordSubmittedText(submission)
    }
}

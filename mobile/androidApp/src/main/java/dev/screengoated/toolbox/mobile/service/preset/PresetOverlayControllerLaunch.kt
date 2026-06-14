package dev.screengoated.toolbox.mobile.service.preset

import android.widget.Toast
import dev.screengoated.toolbox.mobile.preset.AudioPresetLaunchKind
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
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
    if (closePanel) {
        panelModule.dismiss()
    }
    val resolved = presetRepository.getResolvedPreset(presetId) ?: return
    if (audioCaptureSession.toggleOrAbortIfMatching(presetId)) {
        return
    }
    if (!resolved.executionCapability.supported) {
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

        // Capture selected text, then decide flow based on promptMode
        val svc = SgtAccessibilityService.instance
        val treeText = svc?.getSelectedText()
        if (!treeText.isNullOrBlank()) {
            executeTextSelectWithCapturedText(resolved, treeText)
        } else {
            // Click system "Copy" button to put selection into clipboard
            svc?.eagerCaptureSelection()
            processingIndicator.show(uiPreferencesProvider().themeMode, PresetStatusAccent.SUCCESS)

            // Try reading clipboard via accessibility overlay (no visual artifact)
            svc?.readClipboardAsync { overlayResult ->
                if (!overlayResult.isNullOrBlank()) {
                    processingIndicator.dismiss()
                    executeTextSelectWithCapturedText(resolved, overlayResult)
                } else {
                    // Fallback: transparent Activity (brief visual flash)
                    dev.screengoated.toolbox.mobile.service.ClipboardReaderActivity.launch(context) { clipboardText ->
                        processingIndicator.dismiss()
                        executeTextSelectWithCapturedText(resolved, clipboardText)
                    }
                    android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
                        if (processingIndicator.isShowing) {
                            processingIndicator.dismiss()
                            val lang = uiLanguage()
                            val msg = when (lang) {
                                "vi" -> "Hãy copy text trước, sau đó bấm lại preset này"
                                "ko" -> "먼저 텍스트를 복사한 후 이 프리셋을 다시 누르세요"
                                else -> "Copy text first, then tap this preset again"
                            }
                            Toast.makeText(context, msg, Toast.LENGTH_LONG).show()
                        }
                    }, 5000)
                }
            }
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
    val resolved = presetRepository.getResolvedPreset("preset_transcribe") ?: return
    if (!inputModule.hasWindow()) {
        // No input window — normal preset launch
        launchPreset(presetId = resolved.preset.id, closePanel = false, continuousMode = false)
        return
    }
    // Input window is open — mic is just a speech-to-text input method.
    // Record audio, transcribe, inject text into input. Do NOT run the preset pipeline.
    // (matches Windows: show_recording_overlay → set_editor_text, input window stays open)
    if (audioCaptureSession.toggleOrAbortIfMatching(resolved.preset.id)) return
    onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.MICROPHONE)
    audioCaptureSession.start(
        resolvedPreset = resolved,
        onRecordingComplete = { capture ->
            onAudioCaptureForegroundModeChanged(PresetAudioForegroundMode.NONE)
            val transcript = capture.precomputedTranscript
            if (!transcript.isNullOrBlank()) {
                // Streaming transcript is already available from the capture session.
                inputModule.injectText(transcript)
                inputModule.bringToFront()
            } else {
                // Standard runtime (Whisper/Groq) — call transcription API directly
                val audioBlock = resolved.preset.blocks.firstOrNull {
                    it.blockType == dev.screengoated.toolbox.mobile.shared.preset.BlockType.AUDIO
                } ?: return@start
                scope.launch(Dispatchers.Main) {
                    val result = appContainer.audioApiClient.executeStreaming(
                        modelId = audioBlock.model,
                        prompt = audioBlock.resolvePrompt(),
                        wavBytes = capture.wavBytes,
                        apiKeys = buildApiKeys(),
                        uiLanguage = uiLanguage(),
                        onChunk = {},
                    )
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
            handleAudioCaptureFailure(resolved, failure)
        },
    )
}

internal fun PresetOverlayController.resumePendingAudioLaunch() {
    val pending = appContainer.audioPresetLaunchStore.take() ?: return
    if (pending.kind != AudioPresetLaunchKind.CAPTURE) {
        appContainer.audioPresetLaunchStore.set(pending)
        return
    }
    launchPreset(
        presetId = pending.presetId,
        closePanel = false,
        continuousMode = false,
    )
}

internal fun PresetOverlayController.appendStreamingTextChunk(chunk: String): Boolean {
    if (chunk.isBlank()) {
        return false
    }
    val service = SgtAccessibilityService.instance ?: return false
    return service.appendTextToFocusedField(
        text = chunk,
        uiLanguage = uiLanguage(),
    )
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

internal fun PresetOverlayController.submitInput(text: String) {
    val resolved = activePreset ?: return
    val pendingImage = pendingImageBytes
    if (pendingImage != null) {
        // IMAGE + dynamic prompt: inject user's prompt, execute with captured image
        pendingImageBytes = null
        val modifiedPreset = mutateDynamicPromptPreset(resolved, text)
        presetRepository.resetState()
        presetRepository.executePreset(modifiedPreset, PresetInput.Image(pendingImage))
        imageContinuousRearmPending = imageContinuousPresetId == resolved.preset.id
        inputModule.recordSubmittedText(text)
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
        inputModule.recordSubmittedText(text)
    } else {
        // Normal TEXT_INPUT flow
        presetRepository.resetState()
        presetRepository.executePreset(resolved.preset, PresetInput.Text(text))
        inputModule.recordSubmittedText(text)
    }
}

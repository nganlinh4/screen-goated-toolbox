package dev.screengoated.toolbox.mobile.shared.preset

/**
 * Device-audio presets (4).
 *
 * Split out of [DefaultPresets] to keep each category file focused. The list is
 * re-exported via [DefaultPresets] so the public API is unchanged. Block helpers
 * (imageBlock/textBlock/audioBlock/inputAdapter) and model-ID constants are
 * package-level declarations in this same package.
 */
internal val defaultDeviceAudioPresets: List<Preset> = listOf(
    Preset(
        id = "preset_study_language",
        nameEn = "Study language",
        nameVi = "H\u1ecdc ngo\u1ea1i ng\u1eef",
        nameKo = "\uc5b8\uc5b4 \ud559\uc2b5",
        presetType = PresetType.DEVICE_AUDIO,
        audioSource = "device",
        blocks = listOf(
            audioBlock(
                PRESET_AUDIO_TRANSCRIBE_MODEL_ID,
                "",
                "language1" to "Vietnamese",
            ),
            textBlock(
                DEFAULT_TEXT_MODEL_ID,
                "Translate to {language1}. Output ONLY the translation.",
                "language1" to "Vietnamese",
            ),
        ),
    ),

    Preset(
        id = "preset_realtime_audio_translate",
        nameEn = "Live Translate",
        nameVi = "D\u1ecbch cabin",
        nameKo = "\uc2e4\uc2dc\uac04 \uc74c\uc131 \ubc88\uc5ed",
        presetType = PresetType.DEVICE_AUDIO,
        audioSource = "device",
        audioProcessingMode = "realtime",
        blocks = listOf(
            audioBlock(PRESET_AUDIO_TRANSCRIBE_MODEL_ID),
            textBlock(
                "google-gemma-4-26b-a4b-text",
                "",
                "language1" to "Vietnamese",
            ),
        ),
    ),

    Preset(
        id = "preset_record_device",
        nameEn = "Device Record",
        nameVi = "Thu \u00e2m m\u00e1y",
        nameKo = "\uc2dc\uc2a4\ud15c \ub179\uc74c",
        presetType = PresetType.DEVICE_AUDIO,
        audioSource = "device",
        autoStopRecording = true,
        blocks = listOf(
            inputAdapter().copy(showOverlay = true, renderMode = "markdown"),
        ),
    ),

    Preset(
        id = "preset_transcribe_english_offline",
        nameEn = "Transcribe English",
        nameVi = "Ch\u00e9p l\u1eddi TA",
        nameKo = "\uc601\uc5b4 \ubc1b\uc544\uc4f0\uae30",
        presetType = PresetType.DEVICE_AUDIO,
        audioSource = "device",
        autoPaste = true,
        blocks = listOf(
            audioBlock(
                PRESET_AUDIO_OFFLINE_TRANSCRIBE_MODEL_ID,
                "",
                "language1" to "English",
            ).copy(
                showOverlay = false,
                autoCopy = true,
            ),
        ),
    ),
)

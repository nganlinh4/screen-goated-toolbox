package dev.screengoated.toolbox.mobile.shared.preset

/**
 * Microphone presets (8).
 *
 * Split out of [DefaultPresets] to keep each category file focused. The list is
 * re-exported via [DefaultPresets] so the public API is unchanged. Block helpers
 * (imageBlock/textBlock/audioBlock/inputAdapter) and model-ID constants are
 * package-level declarations in this same package.
 */
internal val defaultMicPresets: List<Preset> = listOf(
    Preset(
        id = "preset_transcribe",
        nameEn = "Transcribe speech",
        nameVi = "L\u1eddi n\u00f3i th\u00e0nh v\u0103n",
        nameKo = "\uc74c\uc131 \ubc1b\uc544\uc4f0\uae30",
        presetType = PresetType.MIC,
        autoPaste = true,
        autoStopRecording = true,
        blocks = listOf(
            audioBlock(
                PRESET_AUDIO_TRANSCRIBE_MODEL_ID,
                "Transcribe the audio into text. Output ONLY the transcript.",
                "language1" to "Vietnamese",
            ).copy(
                showOverlay = false,
                renderMode = "markdown",
                autoCopy = true,
            ),
        ),
    ),

    Preset(
        id = "preset_continuous_writing_online",
        nameEn = "Continuous Writing",
        nameVi = "Vi\u1ebft li\u00ean t\u1ee5c",
        nameKo = "\uc5f0\uc18d \uc785\ub825",
        presetType = PresetType.MIC,
        autoPaste = true,
        blocks = listOf(
            audioBlock(
                PRESET_AUDIO_CONTINUOUS_MODEL_ID,
                "",
                "language1" to "Vietnamese",
            ).copy(
                showOverlay = false,
                autoCopy = true,
            ),
        ),
    ),

    Preset(
        id = "preset_fix_pronunciation",
        nameEn = "Fix pronunciation",
        nameVi = "Ch\u1ec9nh ph\u00e1t \u00e2m",
        nameKo = "\ubc1c\uc74c \uad50\uc815",
        presetType = PresetType.MIC,
        autoStopRecording = true,
        blocks = listOf(
            audioBlock(
                PRESET_AUDIO_TRANSCRIBE_MODEL_ID,
                "",
                "language1" to "Vietnamese",
            ).copy(
                showOverlay = false,
                renderMode = "markdown",
                autoSpeak = true,
            ),
        ),
    ),

    Preset(
        id = "preset_transcribe_retranslate",
        nameEn = "Quick 4NR reply 1",
        nameVi = "Tr\u1ea3 l\u1eddi ng.nc.ngo\u00e0i 1",
        nameKo = "\ube60\ub978 \uc678\uad6d\uc778 \ub2f5\ubcc0 1",
        presetType = PresetType.MIC,
        autoPaste = true,
        autoStopRecording = true,
        blocks = listOf(
            audioBlock(
                PRESET_AUDIO_TRANSCRIBE_MODEL_ID,
                "",
                "language1" to "Korean",
            ).copy(showOverlay = false),
            textBlock(
                DEFAULT_TEXT_MODEL_ID,
                "Translate to {language1}. Output ONLY the translation.",
                "language1" to "Korean",
            ).copy(showOverlay = false, autoCopy = true),
        ),
    ),

    Preset(
        id = "preset_quicker_foreigner_reply",
        nameEn = "Quick 4NR reply 2",
        nameVi = "Tr\u1ea3 l\u1eddi ng.nc.ngo\u00e0i 2",
        nameKo = "\ube60\ub978 \uc678\uad6d\uc778 \ub2f5\ubcc0 2",
        presetType = PresetType.MIC,
        autoPaste = true,
        autoStopRecording = true,
        blocks = listOf(
            audioBlock(
                PRESET_AUDIO_DIRECT_TRANSLATE_MODEL_ID,
                "Translate the audio to {language1}. Only output the translated text.",
                "language1" to "Korean",
            ).copy(showOverlay = false, autoCopy = true),
        ),
    ),

    Preset(
        id = "preset_quick_ai_question",
        nameEn = "Quick AI Question",
        nameVi = "H\u1ecfi nhanh AI",
        nameKo = "\ube60\ub978 AI \uc9c8\ubb38",
        presetType = PresetType.MIC,
        autoStopRecording = true,
        blocks = listOf(
            audioBlock(
                PRESET_AUDIO_TRANSCRIBE_MODEL_ID,
                "",
                "language1" to "Vietnamese",
            ).copy(showOverlay = false),
            textBlock(
                DEFAULT_TEXT_MODEL_ID,
                "Answer the following question concisely and helpfully. Format as markdown. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.",
            ),
        ),
    ),

    Preset(
        id = "preset_voice_search",
        nameEn = "Voice Search",
        nameVi = "N\u00f3i \u0111\u1ec3 search",
        nameKo = "\uc74c\uc131 \uac80\uc0c9",
        presetType = PresetType.MIC,
        autoStopRecording = true,
        blocks = listOf(
            audioBlock(
                PRESET_AUDIO_TRANSCRIBE_MODEL_ID,
                "",
                "language1" to "Vietnamese",
            ).copy(showOverlay = false),
            textBlock(
                PRESET_SEARCH_MODEL_ID,
                "Search the internet for information about the following query and provide a comprehensive summary. Include key facts, recent developments, and relevant details with clickable links to sources if possible. Format the output as markdown creatively. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.",
            ),
        ),
    ),

    Preset(
        id = "preset_quick_record",
        nameEn = "Quick Record",
        nameVi = "Thu \u00e2m nhanh",
        nameKo = "\ube60\ub978 \ub179\uc74c",
        presetType = PresetType.MIC,
        autoStopRecording = true,
        blocks = listOf(
            inputAdapter().copy(showOverlay = true, renderMode = "markdown"),
        ),
    ),
)

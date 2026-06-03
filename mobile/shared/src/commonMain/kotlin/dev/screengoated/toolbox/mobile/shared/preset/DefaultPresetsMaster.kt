package dev.screengoated.toolbox.mobile.shared.preset

/**
 * Master presets (5).
 *
 * Split out of [DefaultPresets] to keep each category file focused. The list is
 * re-exported via [DefaultPresets] so the public API is unchanged. Block helpers
 * (imageBlock/textBlock/audioBlock/inputAdapter) and model-ID constants are
 * package-level declarations in this same package.
 */
internal val defaultMasterPresets: List<Preset> = listOf(
    Preset(
        id = "preset_image_master",
        nameEn = "Image MASTER",
        nameVi = "\u1ea2nh MASTER",
        nameKo = "\uc774\ubbf8\uc9c0 \ub9c8\uc2a4\ud130",
        presetType = PresetType.IMAGE,
        isMaster = true,
        showControllerUi = true,
        isUpcoming = true,
        blocks = emptyList(),
    ),

    Preset(
        id = "preset_text_select_master",
        nameEn = "Select MASTER",
        nameVi = "B\u00f4i MASTER",
        nameKo = "\uc120\ud0dd \ub9c8\uc2a4\ud130",
        presetType = PresetType.TEXT_SELECT,
        isMaster = true,
        showControllerUi = true,
        blocks = emptyList(),
    ),

    Preset(
        id = "preset_text_type_master",
        nameEn = "Type MASTER",
        nameVi = "G\u00f5 MASTER",
        nameKo = "\uc785\ub825 \ub9c8\uc2a4\ud130",
        presetType = PresetType.TEXT_INPUT,
        isMaster = true,
        showControllerUi = true,
        blocks = emptyList(),
    ),

    Preset(
        id = "preset_audio_mic_master",
        nameEn = "Mic MASTER",
        nameVi = "Mic MASTER",
        nameKo = "\ub9c8\uc774\ud06c \ub9c8\uc2a4\ud130",
        presetType = PresetType.MIC,
        isMaster = true,
        showControllerUi = true,
        autoStopRecording = true,
        blocks = emptyList(),
    ),

    Preset(
        id = "preset_audio_device_master",
        nameEn = "Sound MASTER",
        nameVi = "Ti\u1ebfng MASTER",
        nameKo = "\uc0ac\uc6b4\ub4dc \ub9c8\uc2a4\ud130",
        presetType = PresetType.DEVICE_AUDIO,
        isMaster = true,
        showControllerUi = true,
        blocks = emptyList(),
    ),
)

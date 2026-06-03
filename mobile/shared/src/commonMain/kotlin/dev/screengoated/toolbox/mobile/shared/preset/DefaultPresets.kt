package dev.screengoated.toolbox.mobile.shared.preset

/**
 * All built-in presets ported from the Windows desktop app.
 *
 * Preset IDs, prompt strings, model IDs, language variables, auto-behaviors,
 * and block connections are exact copies of the Rust defaults in
 * `src/config/preset/defaults/{image,text,audio,master}.rs`.
 *
 * Localized names come from `src/gui/settings_ui/sidebar.rs`
 * (`get_localized_preset_name`).
 *
 * The presets themselves live in per-category sibling files
 * (`DefaultPresets<Category>.kt`); this object only aggregates and re-exports
 * them so the public surface (`DefaultPresets.imagePresets`, `DefaultPresets.all`,
 * …) stays stable. Block helpers and model-ID constants are package-level
 * declarations in [PresetModels] and the generated model-ID file.
 */
object DefaultPresets {

    /** Image presets (16). See `DefaultPresetsImage.kt`. */
    val imagePresets: List<Preset> = defaultImagePresets

    /** Text-selection presets (13). See `DefaultPresetsTextSelect.kt`. */
    val textSelectPresets: List<Preset> = defaultTextSelectPresets

    /** Text-input (typing) presets (5). See `DefaultPresetsTextInput.kt`. */
    val textInputPresets: List<Preset> = defaultTextInputPresets

    /** Microphone presets (8). See `DefaultPresetsMic.kt`. */
    val micPresets: List<Preset> = defaultMicPresets

    /** Device-audio presets (4). See `DefaultPresetsDeviceAudio.kt`. */
    val deviceAudioPresets: List<Preset> = defaultDeviceAudioPresets

    /** Master presets (5). See `DefaultPresetsMaster.kt`. */
    val masterPresets: List<Preset> = defaultMasterPresets

    /** Every built-in preset, in canonical section order. */
    val all: List<Preset> =
        imagePresets + textSelectPresets + textInputPresets +
            micPresets + deviceAudioPresets + masterPresets
}

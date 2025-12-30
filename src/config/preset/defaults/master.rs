//! Default MASTER presets using the builder pattern.
//!
//! MASTER presets are special presets that show a preset wheel for selection
//! instead of directly processing input.

use crate::config::preset::Preset;
use crate::config::preset::PresetBuilder;

/// Create all default MASTER presets
pub fn create_master_presets() -> Vec<Preset> {
    vec![
        // Image MASTER
        PresetBuilder::new("preset_image_master", "Image MASTER")
            .image()
            .master()
            .build(),
        // Text-Select MASTER
        PresetBuilder::new("preset_text_select_master", "Text-Select MASTER")
            .text_select()
            .master()
            .build(),
        // Text-Type MASTER
        PresetBuilder::new("preset_text_type_master", "Text-Type MASTER")
            .text_type()
            .master()
            .build(),
        // Mic MASTER
        {
            let mut p = PresetBuilder::new("preset_audio_mic_master", "Mic MASTER")
                .audio_mic()
                .master()
                .build();
            p.auto_stop_recording = true; // MASTER presets keep this setting
            p
        },
        // Device Audio MASTER
        PresetBuilder::new("preset_audio_device_master", "Device Audio MASTER")
            .audio_device()
            .master()
            .build(),
    ]
}

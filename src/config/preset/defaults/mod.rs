//! Default presets module.
//!
//! This module provides all built-in presets in a single ordered list.
//! The order here determines the default display order in the UI.

mod audio;
mod image;
mod master;
mod text;

use crate::config::preset::Preset;

pub use audio::create_audio_presets;
pub use image::create_image_presets;
pub use master::create_master_presets;
pub use text::create_text_presets;

/// Get all default presets in the correct display order.
///
/// The order is organized by columns in the sidebar:
/// - Column 1: Image presets, ending with Image MASTER
/// - Column 2: Text-Select presets, Text-Type presets, with MASTERs at section ends
/// - Column 3: Mic presets, Device Audio presets, with MASTERs at section ends
pub fn get_default_presets() -> Vec<Preset> {
    let image = create_image_presets();
    let text = create_text_presets();
    let audio = create_audio_presets();
    let masters = create_master_presets();

    // Helper to find preset by ID
    let find = |presets: &[Preset], id: &str| -> Preset {
        presets.iter().find(|p| p.id == id).cloned().unwrap()
    };

    vec![
        // =====================================================================
        // COLUMN 1: IMAGE PRESETS
        // =====================================================================
        find(&image, "preset_translate"),
        find(&image, "preset_extract_retranslate"),
        find(&image, "preset_translate_auto_paste"),
        find(&image, "preset_extract_table"),
        find(&image, "preset_translate_retranslate"),
        find(&image, "preset_extract_retrans_retrans"),
        find(&image, "preset_ocr"),
        find(&image, "preset_ocr_read"),
        find(&image, "preset_quick_screenshot"),
        find(&image, "preset_qr_scanner"),
        find(&image, "preset_summarize"),
        find(&image, "preset_desc"),
        find(&image, "preset_ask_image"),
        find(&image, "preset_fact_check"),
        find(&image, "preset_omniscient_god"),
        find(&image, "preset_hang_image"),
        find(&masters, "preset_image_master"),
        // =====================================================================
        // COLUMN 2: TEXT PRESETS
        // =====================================================================
        // Text-Select section
        find(&text, "preset_read_aloud"),
        find(&text, "preset_translate_select"),
        find(&text, "preset_translate_arena"),
        find(&text, "preset_trans_retrans_select"),
        find(&text, "preset_select_translate_replace"),
        find(&text, "preset_fix_grammar"),
        find(&text, "preset_rephrase"),
        find(&text, "preset_make_formal"),
        find(&text, "preset_explain"),
        find(&text, "preset_ask_text"),
        find(&text, "preset_edit_as_follows"),
        find(&text, "preset_hang_text"),
        find(&masters, "preset_text_select_master"),
        // Text-Type section
        find(&text, "preset_trans_retrans_typing"),
        find(&text, "preset_ask_ai"),
        find(&text, "preset_internet_search"),
        find(&text, "preset_make_game"),
        find(&text, "preset_quick_note"),
        find(&masters, "preset_text_type_master"),
        // =====================================================================
        // COLUMN 3: AUDIO PRESETS
        // =====================================================================
        // Mic section
        find(&audio, "preset_transcribe"),
        find(&audio, "preset_fix_pronunciation"),
        find(&audio, "preset_transcribe_retranslate"),
        find(&audio, "preset_quicker_foreigner_reply"),
        find(&audio, "preset_quick_ai_question"),
        find(&audio, "preset_voice_search"),
        find(&audio, "preset_quick_record"),
        find(&masters, "preset_audio_mic_master"),
        // Device audio section
        find(&audio, "preset_study_language"),
        find(&audio, "preset_record_device"),
        find(&masters, "preset_audio_device_master"),
        find(&audio, "preset_realtime_audio_translate"),
    ]
}

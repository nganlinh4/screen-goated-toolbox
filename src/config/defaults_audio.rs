//! Default presets - Part 3: Audio and Master presets

use super::preset::Preset;
use super::types::ProcessingBlock;

/// Create audio-based default presets
pub fn create_audio_presets() -> Vec<Preset> {
    let mut presets = Vec::new();
    
    // 11. Transcribe (Audio)
    let mut p11 = Preset::default();
    p11.id = "preset_transcribe".to_string();
    p11.name = "Transcribe speech".to_string();
    p11.preset_type = "audio".to_string();
    p11.audio_source = "mic".to_string();
    p11.auto_paste = true;
    p11.auto_stop_recording = true;
    p11.blocks = vec![
        ProcessingBlock {
            block_type: "audio".to_string(),
            model: "whisper-accurate".to_string(),
            prompt: "".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: false,
            show_overlay: false,
            auto_copy: true,
            ..Default::default()
        }
    ];
    presets.push(p11);

    // 11b. Fix pronunciation (Chỉnh phát âm)
    let mut p11b = Preset::default();
    p11b.id = "preset_fix_pronunciation".to_string();
    p11b.name = "Fix pronunciation".to_string();
    p11b.preset_type = "audio".to_string();
    p11b.audio_source = "mic".to_string();
    p11b.auto_paste = false;
    p11b.auto_stop_recording = true;
    p11b.blocks = vec![
        ProcessingBlock {
            block_type: "audio".to_string(),
            model: "whisper-accurate".to_string(),
            prompt: "".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: false,
            show_overlay: false,
            auto_copy: false,
            auto_speak: true,
            ..Default::default()
        }
    ];
    presets.push(p11b);

    // 12. Study language Preset
    let mut p12 = Preset::default();
    p12.id = "preset_study_language".to_string();
    p12.name = "Study language".to_string();
    p12.preset_type = "audio".to_string();
    p12.audio_source = "device".to_string();
    p12.blocks = vec![
        ProcessingBlock {
            block_type: "audio".to_string(),
            model: "whisper-accurate".to_string(),
            prompt: "".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: true,
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        },
        ProcessingBlock {
            block_type: "text".to_string(),
            model: "text_accurate_kimi".to_string(),
            prompt: "Translate to {language1}. Output ONLY the translation.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: true,
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        }
    ];
    presets.push(p12);

    // 13. Quick 4NR reply
    let mut p13 = Preset::default();
    p13.id = "preset_transcribe_retranslate".to_string();
    p13.name = "Quick 4NR reply".to_string();
    p13.preset_type = "audio".to_string();
    p13.audio_source = "mic".to_string();
    p13.auto_paste = true;
    p13.blocks = vec![
        ProcessingBlock {
            block_type: "audio".to_string(),
            model: "whisper-accurate".to_string(),
            prompt: "".to_string(),
            selected_language: "Korean".to_string(),
            streaming_enabled: false,
            show_overlay: false,
            auto_copy: false,
            ..Default::default()
        },
        ProcessingBlock {
            block_type: "text".to_string(),
            model: "text_accurate_kimi".to_string(),
            prompt: "Translate to {language1}. Output ONLY the translation.".to_string(),
            selected_language: "Korean".to_string(),
            streaming_enabled: true,
            show_overlay: false,
            auto_copy: true,
            ..Default::default()
        }
    ];
    presets.push(p13);

    // 14. Quicker foreigner reply Preset
    let mut p14 = Preset::default();
    p14.id = "preset_quicker_foreigner_reply".to_string();
    p14.name = "Quicker foreigner reply".to_string();
    p14.preset_type = "audio".to_string();
    p14.audio_source = "mic".to_string();
    p14.auto_paste = true;
    p14.blocks = vec![
        ProcessingBlock {
            block_type: "audio".to_string(),
            model: "gemini-audio".to_string(),
            prompt: "Translate the audio to {language1}. Only output the translated text.".to_string(),
            selected_language: "Korean".to_string(),
            streaming_enabled: false,
            show_overlay: false,
            auto_copy: true,
            ..Default::default()
        }
    ];
    presets.push(p14);

    // 16. Live Translation (Dịch cabin) Placeholder
    let mut p16 = Preset::default();
    p16.id = "preset_realtime_audio_translate".to_string();
    p16.name = "Live Translate".to_string();
    p16.preset_type = "audio".to_string();
    p16.audio_source = "device".to_string(); // Device audio for cabin translation
    p16.audio_processing_mode = "realtime".to_string();
    p16.is_upcoming = false;
    p16.blocks = vec![
        ProcessingBlock {
            block_type: "audio".to_string(),
            model: "whisper-accurate".to_string(),
            auto_copy: false,
            ..Default::default()
        },
        ProcessingBlock {
            block_type: "text".to_string(),
            model: "google-gemma".to_string(),
            selected_language: "Vietnamese".to_string(),
            auto_copy: false,
            ..Default::default()
        }
    ];
    presets.push(p16);

    // 16b. Hỏi nhanh AI (Quick AI question via mic)
    let mut p16b = Preset::default();
    p16b.id = "preset_quick_ai_question".to_string();
    p16b.name = "Quick AI Question".to_string();
    p16b.preset_type = "audio".to_string();
    p16b.audio_source = "mic".to_string();
    p16b.auto_stop_recording = true;
    p16b.blocks = vec![
        ProcessingBlock {
            block_type: "audio".to_string(),
            model: "whisper-accurate".to_string(),
            prompt: "".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: false,
            show_overlay: false,
            auto_copy: false,
            ..Default::default()
        },
        ProcessingBlock {
            block_type: "text".to_string(),
            model: "text_accurate_kimi".to_string(),
            prompt: "Answer the following question concisely and helpfully. Format as markdown. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.".to_string(),
            streaming_enabled: true,
            render_mode: "markdown".to_string(),
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        }
    ];
    presets.push(p16b);

    // 16c. Nói để search (Speak to search)
    let mut p16c = Preset::default();
    p16c.id = "preset_voice_search".to_string();
    p16c.name = "Voice Search".to_string();
    p16c.preset_type = "audio".to_string();
    p16c.audio_source = "mic".to_string();
    p16c.auto_stop_recording = true;
    p16c.blocks = vec![
        ProcessingBlock {
            block_type: "audio".to_string(),
            model: "whisper-accurate".to_string(),
            prompt: "".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: false,
            show_overlay: false,
            auto_copy: false,
            ..Default::default()
        },
        ProcessingBlock {
            block_type: "text".to_string(),
            model: "compound_mini".to_string(),
            prompt: "Search the internet for information about the following query and provide a comprehensive summary. Include key facts, recent developments, and relevant details with clickable links to sources if possible. Format the output as markdown creatively. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.".to_string(),
            streaming_enabled: true,
            render_mode: "markdown".to_string(),
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        }
    ];
    presets.push(p16c);

    presets
}

/// Create MASTER presets (preset wheels)
pub fn create_master_presets() -> Vec<Preset> {
    let mut presets = Vec::new();
    
    // 17. Image MASTER (Ảnh MASTER)
    let mut pm1 = Preset::default();
    pm1.id = "preset_image_master".to_string();
    pm1.name = "Image MASTER".to_string();
    pm1.preset_type = "image".to_string();
    pm1.is_master = true;
    pm1.show_controller_ui = true;
    pm1.blocks = vec![]; // MASTER presets don't have their own blocks
    presets.push(pm1);

    // 18. Text-Select MASTER (Bôi MASTER)
    let mut pm2 = Preset::default();
    pm2.id = "preset_text_select_master".to_string();
    pm2.name = "Text-Select MASTER".to_string();
    pm2.preset_type = "text".to_string();
    pm2.text_input_mode = "select".to_string();
    pm2.is_master = true;
    pm2.show_controller_ui = true;
    pm2.blocks = vec![];
    presets.push(pm2);

    // 19. Text-Type MASTER (Gõ MASTER)
    let mut pm3 = Preset::default();
    pm3.id = "preset_text_type_master".to_string();
    pm3.name = "Text-Type MASTER".to_string();
    pm3.preset_type = "text".to_string();
    pm3.text_input_mode = "type".to_string();
    pm3.is_master = true;
    pm3.show_controller_ui = true;
    pm3.blocks = vec![];
    presets.push(pm3);

    // 20. Mic MASTER (Mic MASTER)
    let mut pm4 = Preset::default();
    pm4.id = "preset_audio_mic_master".to_string();
    pm4.name = "Mic MASTER".to_string();
    pm4.preset_type = "audio".to_string();
    pm4.audio_source = "mic".to_string();
    pm4.auto_stop_recording = true;
    pm4.is_master = true;
    pm4.show_controller_ui = true;
    pm4.blocks = vec![];
    presets.push(pm4);

    // 21. Device Audio MASTER (Tiếng MASTER)
    let mut pm5 = Preset::default();
    pm5.id = "preset_audio_device_master".to_string();
    pm5.name = "Device Audio MASTER".to_string();
    pm5.preset_type = "audio".to_string();
    pm5.audio_source = "device".to_string();
    pm5.is_master = true;
    pm5.show_controller_ui = true;
    pm5.blocks = vec![];
    presets.push(pm5);

    presets
}

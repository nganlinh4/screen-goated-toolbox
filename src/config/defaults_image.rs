//! Default presets - Part 1: Image and Text presets

use super::preset::Preset;
use super::types::{Hotkey, ProcessingBlock};

/// Create image-based default presets
pub fn create_image_presets() -> Vec<Preset> {
    let mut presets = Vec::new();

    // 1. Standard Translate Preset (Image -> Text)
    let mut p1 = Preset::default();
    p1.id = "preset_translate".to_string();
    p1.name = "Translate".to_string();
    p1.preset_type = "image".to_string();
    p1.blocks = vec![
        ProcessingBlock {
            block_type: "image".to_string(),
            model: "maverick".to_string(),
            prompt: "Extract text from this image and translate it to {language1}. Output ONLY the translation text directly, do not add introductory text.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: false,
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        }
    ];
    presets.push(p1);

    // 2. Translate (Auto paste) Preset
    let mut p2 = Preset::default();
    p2.id = "preset_translate_auto_paste".to_string();
    p2.name = "Translate (Auto paste)".to_string();
    p2.preset_type = "image".to_string();
    p2.auto_paste = true;
    p2.blocks = vec![
        ProcessingBlock {
            block_type: "image".to_string(),
            model: "maverick".to_string(),
            prompt: "Extract text from this image and translate it to {language1}. Output ONLY the translation text directly, do not add introductory text.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: false,
            show_overlay: false,
            auto_copy: true,
            ..Default::default()
        }
    ];
    presets.push(p2);

    // 3g. Extract Table (Trích bảng) - IMAGE preset
    let mut p3g = Preset::default();
    p3g.id = "preset_extract_table".to_string();
    p3g.name = "Extract Table".to_string();
    p3g.preset_type = "image".to_string();
    p3g.blocks = vec![
        ProcessingBlock {
            block_type: "image".to_string(),
            model: "maverick".to_string(),
            prompt: "Extract all data from any tables, forms, or structured content in this image. Format the output as a markdown table. Output ONLY the table, no explanations.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: false,
            render_mode: "markdown".to_string(),
            show_overlay: true,
            auto_copy: true,
            ..Default::default()
        }
    ];
    presets.push(p3g);

    // 4. Chain: OCR -> Translate
    let mut p4 = Preset::default();
    p4.id = "preset_translate_retranslate".to_string();
    p4.name = "Translate+Retranslate".to_string();
    p4.preset_type = "image".to_string();
    p4.blocks = vec![
        ProcessingBlock {
            block_type: "image".to_string(),
            model: "maverick".to_string(),
            prompt: "Extract text from this image and translate it to {language1}. Output ONLY the translation text directly, do not add introductory text.".to_string(),
            selected_language: "Korean".to_string(),
            streaming_enabled: false,
            show_overlay: true,
            auto_copy: true,
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
    presets.push(p4);

    // 4b. Chain: OCR (Accurate) -> Translate Korean -> Translate Vietnamese
    let mut p4b = Preset::default();
    p4b.id = "preset_extract_retrans_retrans".to_string();
    p4b.name = "Translate (Accurate)+Retranslate".to_string();
    p4b.preset_type = "image".to_string();
    p4b.blocks = vec![
        ProcessingBlock {
            block_type: "image".to_string(),
            model: "maverick".to_string(),
            prompt: "Extract all text from this image exactly as it appears. Output ONLY the text."
                .to_string(),
            selected_language: "English".to_string(),
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
            show_overlay: true,
            auto_copy: true,
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
        },
    ];
    presets.push(p4b);

    // 6. OCR Preset
    let mut p6 = Preset::default();
    p6.id = "preset_ocr".to_string();
    p6.name = "Extract text".to_string();
    p6.preset_type = "image".to_string();
    p6.blocks = vec![ProcessingBlock {
        block_type: "image".to_string(),
        model: "scout".to_string(),
        prompt: "Extract all text from this image exactly as it appears. Output ONLY the text."
            .to_string(),
        selected_language: "English".to_string(),
        streaming_enabled: false,
        show_overlay: false,
        auto_copy: true,
        ..Default::default()
    }];
    presets.push(p6);

    // 6c. Quick Screenshot
    let mut p6c = Preset::default();
    p6c.id = "preset_quick_screenshot".to_string();
    p6c.name = "Quick Screenshot".to_string();
    p6c.preset_type = "image".to_string();
    p6c.blocks = vec![ProcessingBlock {
        block_type: "input_adapter".to_string(), // Explicit input adapter for single-node graph
        auto_copy: true,
        ..Default::default()
    }];
    presets.push(p6c);

    // 7. Translate (High accuracy)
    let mut p7 = Preset::default();
    p7.id = "preset_extract_retranslate".to_string();
    p7.name = "Translate (High accuracy)".to_string();
    p7.preset_type = "image".to_string();
    p7.blocks = vec![
        ProcessingBlock {
            block_type: "image".to_string(),
            model: "maverick".to_string(),
            prompt: "Extract all text from this image exactly as it appears. Output ONLY the text."
                .to_string(),
            selected_language: "English".to_string(),
            streaming_enabled: false,
            show_overlay: false,
            auto_copy: false,
            ..Default::default()
        },
        ProcessingBlock {
            block_type: "text".to_string(),
            model: "text_accurate_kimi".to_string(),
            prompt: "Translate to {language1}. Output ONLY the translation.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: false,
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        },
    ];
    p7.hotkeys.push(Hotkey {
        code: 192,
        name: "` / ~".to_string(),
        modifiers: 0,
    });
    presets.push(p7);

    // 6b. OCR Read (Đọc vùng này)
    let mut p6b = Preset::default();
    p6b.id = "preset_ocr_read".to_string();
    p6b.name = "Read this region".to_string();
    p6b.preset_type = "image".to_string();
    p6b.blocks = vec![ProcessingBlock {
        block_type: "image".to_string(),
        model: "maverick".to_string(),
        prompt: "Extract all text from this image exactly as it appears. Output ONLY the text."
            .to_string(),
        selected_language: "English".to_string(),
        streaming_enabled: false,
        show_overlay: false,
        auto_copy: false,
        auto_speak: true,
        ..Default::default()
    }];
    presets.push(p6b);

    // 8. Summarize Preset
    let mut p8 = Preset::default();
    p8.id = "preset_summarize".to_string();
    p8.name = "Summarize content".to_string();
    p8.preset_type = "image".to_string();
    p8.blocks = vec![
        ProcessingBlock {
            block_type: "image".to_string(),
            model: "maverick".to_string(),
            prompt: "Analyze this image and summarize its content in {language1}. Only return the summary text, super concisely. Format the output as a markdown. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: true,
            render_mode: "markdown".to_string(),
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        }
    ];
    presets.push(p8);

    // 9. Image description Preset
    let mut p9 = Preset::default();
    p9.id = "preset_desc".to_string();
    p9.name = "Image description".to_string();
    p9.preset_type = "image".to_string();
    p9.blocks = vec![
        ProcessingBlock {
            block_type: "image".to_string(),
            model: "maverick".to_string(),
            prompt: "Describe this image in {language1}. Format the output as a markdown. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: true,
            render_mode: "markdown".to_string(),
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        }
    ];
    presets.push(p9);

    // 10. Ask about image
    let mut p10 = Preset::default();
    p10.id = "preset_ask_image".to_string();
    p10.name = "Ask about image".to_string();
    p10.preset_type = "image".to_string();
    p10.prompt_mode = "dynamic".to_string();
    p10.blocks = vec![ProcessingBlock {
        block_type: "image".to_string(),
        model: "gemini-pro".to_string(),
        prompt: "".to_string(),
        selected_language: "Vietnamese".to_string(),
        streaming_enabled: true,
        render_mode: "markdown".to_string(),
        show_overlay: true,
        auto_copy: false,
        ..Default::default()
    }];
    presets.push(p10);

    // 14b. Kiểm chứng thông tin (Fact Check) - IMAGE preset with chain
    let mut p14b = Preset::default();
    p14b.id = "preset_fact_check".to_string();
    p14b.name = "Kiểm chứng thông tin".to_string();
    p14b.preset_type = "image".to_string();
    p14b.blocks = vec![
        ProcessingBlock {
            block_type: "image".to_string(),
            model: "maverick".to_string(),
            prompt: "Extract and describe all text, claims, statements, and information visible in this image. Include any context that might be relevant for fact-checking. Output the content clearly.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: false,
            show_overlay: false,
            auto_copy: false,
            ..Default::default()
        },
        ProcessingBlock {
            block_type: "text".to_string(),
            model: "compound_mini".to_string(),
            prompt: "Fact-check the following claims/information. Search the internet to verify accuracy. Provide a clear verdict (TRUE/FALSE/PARTIALLY TRUE/UNVERIFIABLE) for each claim with evidence and sources. Respond in {language1}. Format as markdown. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: true,
            render_mode: "markdown".to_string(),
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        }
    ];
    presets.push(p14b);

    // 14c. Thần Trí tuệ (Omniscient God)
    let mut p14c = Preset::default();
    p14c.id = "preset_omniscient_god".to_string();
    p14c.name = "Thần Trí tuệ (Omniscient God)".to_string();
    p14c.preset_type = "image".to_string();
    p14c.blocks = vec![
        // Node 1 (0): 
        ProcessingBlock {
            block_type: "image".to_string(),
            model: "maverick".to_string(),
            prompt: "Analyze this image and extract all text, claims, and key information. Be detailed and comprehensive.".to_string(),
            selected_language: "English".to_string(),
            streaming_enabled: false,
            render_mode: "markdown".to_string(),
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        },
        // Node 4 (3 -> 1): Make a learning HTML (1->4)
        ProcessingBlock {
            block_type: "text".to_string(),
            model: "text_accurate_kimi".to_string(),
            prompt: "Create a standalone INTERACTIVE HTML learning card/game for the following text. Use internal CSS for a beautiful, modern, colored design, game-like and comprehensive interface. Only OUTPUT the raw HTML code, DO NOT include HTML file indicator (```html) or triple backticks.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: true,
            render_mode: "markdown".to_string(),
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        },
        // Node 3 (2): Summarize into markdown (2->3)
        ProcessingBlock {
            block_type: "text".to_string(),
            model: "compound_mini".to_string(),
            prompt: "Search the internet to ensure of the accuracy of the following text as well as getting as much source information as possible. Summarize the following text into a detailed markdown summary with clickable links to the sources. Structure it clearly. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: true,
            render_mode: "markdown".to_string(),
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        },
        // Node 2 (1 -> 3): Translate (1->2)
        ProcessingBlock {
            block_type: "text".to_string(),
            model: "text_accurate_kimi".to_string(),
            prompt: "Translate the following text to {language1}. Output ONLY the translation.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: true,
            render_mode: "markdown".to_string(),
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        },
         // Node 5 (4): Summarize into several words (2->5)
        ProcessingBlock {
            block_type: "text".to_string(),
            model: "text_accurate_kimi".to_string(),
            prompt: "Summarize the essence of this text into 3-5 keywords or a short phrase in {language1}.".to_string(),
            selected_language: "Vietnamese".to_string(),
            streaming_enabled: true,
            show_overlay: true,
            auto_copy: false,
            ..Default::default()
        }
    ];
    p14c.block_connections = vec![(0, 3), (0, 1), (3, 4), (3, 2)];
    presets.push(p14c);

    presets
}

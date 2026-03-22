package dev.screengoated.toolbox.mobile.shared.preset

/**
 * All 52 built-in presets ported from the Windows desktop app.
 *
 * Preset IDs, prompt strings, model IDs, language variables, auto-behaviors,
 * and block connections are exact copies of the Rust defaults in
 * `src/config/preset/defaults/{image,text,audio,master}.rs`.
 *
 * Localized names come from `src/gui/settings_ui/sidebar.rs`
 * (`get_localized_preset_name`).
 */
/** Default image-to-text model ID — mirrors `DEFAULT_IMAGE_MODEL_ID` in Rust `model_config.rs`. */
const val DEFAULT_IMAGE_MODEL_ID = "scout"

object DefaultPresets {

    // =====================================================================
    // IMAGE PRESETS (16 presets)
    // =====================================================================

    val imagePresets: List<Preset> = listOf(
        // -- Translation -----------------------------------------------

        Preset(
            id = "preset_translate",
            nameEn = "Translate region",
            nameVi = "D\u1ecbch v\u00f9ng",
            nameKo = "\uc601\uc5ed \ubc88\uc5ed",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                imageBlock(
                    DEFAULT_IMAGE_MODEL_ID,
                    "Extract text from this image and translate it to {language1}. Output ONLY the translation text directly, do not add introductory text.",
                    "language1" to "Vietnamese",
                ),
            ),
        ),

        Preset(
            id = "preset_extract_retranslate",
            nameEn = "Trans reg (ACCURATE)",
            nameVi = "D\u1ecbch v\u00f9ng (CHU\u1ea8N)",
            nameKo = "\uc601\uc5ed \ubc88\uc5ed (\uc815\ud655)",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                imageBlock(
                    DEFAULT_IMAGE_MODEL_ID,
                    "Extract all text from this image exactly as it appears. Output ONLY the text.",
                    "language1" to "English",
                ).copy(showOverlay = false),
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate to {language1}. Output ONLY the translation.",
                    "language1" to "Vietnamese",
                ).copy(streamingEnabled = false, renderMode = "markdown"),
            ),
        ),

        Preset(
            id = "preset_translate_auto_paste",
            nameEn = "Trans reg (Auto paste)",
            nameVi = "D\u1ecbch v\u00f9ng (T\u1ef1 d\u00e1n)",
            nameKo = "\uc601\uc5ed \ubc88\uc5ed (\uc790\ub3d9 \ubd99.)",
            presetType = PresetType.IMAGE,
            autoPaste = true,
            blocks = listOf(
                imageBlock(
                    DEFAULT_IMAGE_MODEL_ID,
                    "Extract text from this image and translate it to {language1}. Output ONLY the translation text directly, do not add introductory text.",
                    "language1" to "Vietnamese",
                ).copy(showOverlay = false, autoCopy = true),
            ),
        ),

        Preset(
            id = "preset_translate_retranslate",
            nameEn = "Trans reg+Retrans",
            nameVi = "D\u1ecbch v\u00f9ng+D\u1ecbch l\u1ea1i",
            nameKo = "\uc601\uc5ed \ubc88\uc5ed+\uc7ac\ubc88\uc5ed",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                imageBlock(
                    DEFAULT_IMAGE_MODEL_ID,
                    "Extract text from this image and translate it to {language1}. Output ONLY the translation text directly, do not add introductory text.",
                    "language1" to "Korean",
                ).copy(renderMode = "markdown", autoCopy = true),
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate to {language1}. Output ONLY the translation.",
                    "language1" to "Vietnamese",
                ),
            ),
        ),

        Preset(
            id = "preset_extract_retrans_retrans",
            nameEn = "Trans (ACC)+Retrans",
            nameVi = "D.v\u00f9ng (CHU\u1ea8N)+D.l\u1ea1i",
            nameKo = "\uc601.\ubc88\uc5ed (\uc815\ud655)+\uc7ac\ubc88\uc5ed",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                imageBlock(
                    DEFAULT_IMAGE_MODEL_ID,
                    "Extract all text from this image exactly as it appears. Output ONLY the text.",
                    "language1" to "English",
                ).copy(showOverlay = false),
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate to {language1}. Output ONLY the translation.",
                    "language1" to "Korean",
                ).copy(autoCopy = true),
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate to {language1}. Output ONLY the translation.",
                    "language1" to "Vietnamese",
                ),
            ),
        ),

        // -- Extraction ------------------------------------------------

        Preset(
            id = "preset_ocr",
            nameEn = "Extract text",
            nameVi = "L\u1ea5y text t\u1eeb \u1ea3nh",
            nameKo = "\ud14d\uc2a4\ud2b8 \ucd94\ucd9c",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                imageBlock(
                    DEFAULT_IMAGE_MODEL_ID,
                    "Extract all text from this image exactly as it appears. Output ONLY the text.",
                    "language1" to "English",
                ).copy(showOverlay = false, renderMode = "markdown", autoCopy = true),
            ),
        ),

        Preset(
            id = "preset_ocr_read",
            nameEn = "Read this region",
            nameVi = "\u0110\u1ecdc v\u00f9ng n\u00e0y",
            nameKo = "\uc601\uc5ed \uc77d\uae30",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                imageBlock(
                    DEFAULT_IMAGE_MODEL_ID,
                    "Extract all text from this image exactly as it appears. Output ONLY the text.",
                    "language1" to "English",
                ).copy(showOverlay = false, renderMode = "markdown", autoSpeak = true),
            ),
        ),

        Preset(
            id = "preset_quick_screenshot",
            nameEn = "Quick screenshot",
            nameVi = "Ch\u1ee5p MH nhanh",
            nameKo = "\ube60\ub978 \uc2a4\ud06c\ub9b0\uc0f7",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                inputAdapter().copy(autoCopy = true),
            ),
        ),

        Preset(
            id = "preset_extract_table",
            nameEn = "Extract Table",
            nameVi = "Tr\u00edch b\u1ea3ng",
            nameKo = "\ud45c \ucd94\ucd9c",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                imageBlock(
                    DEFAULT_IMAGE_MODEL_ID,
                    "Extract all data from any tables, forms, or structured content in this image. Format the output as a markdown table. Output ONLY the table, no explanations.",
                    "language1" to "Vietnamese",
                ).copy(renderMode = "markdown", autoCopy = true),
            ),
        ),

        Preset(
            id = "preset_qr_scanner",
            nameEn = "QR Scanner",
            nameVi = "Qu\u00e9t m\u00e3 QR",
            nameKo = "QR \uc2a4\uce94",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                imageBlock(
                    "qr-scanner",
                    "",
                ).copy(showOverlay = false, autoCopy = true),
                textBlock(
                    "cerebras_gpt_oss",
                    "Format this QR code content for display. Rules:\n" +
                        "- If URL: Make it a clickable markdown link [URL](URL) and describe what this link points to\n" +
                        "- If vCard/contact: Format as a readable contact card with name, phone, email, address\n" +
                        "- If WiFi (WIFI:S:...): Extract and display SSID, password, and security type clearly\n" +
                        "- If plain text: Display as-is, translate if not in {language1}\n" +
                        "- If calendar event: Format as readable event with date/time/location\n" +
                        "- If email/SMS: Format with recipient and content clearly\n" +
                        "Output clean markdown. DO NOT include code blocks or backticks.",
                    "language1" to "Vietnamese",
                ),
            ),
        ),

        // -- Analysis --------------------------------------------------

        Preset(
            id = "preset_summarize",
            nameEn = "Summarize region",
            nameVi = "T\u00f3m t\u1eaft v\u00f9ng",
            nameKo = "\uc601\uc5ed \uc694\uc57d",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                imageBlock(
                    DEFAULT_IMAGE_MODEL_ID,
                    "Analyze this image and summarize its content in {language1}. Only return the summary text, super concisely. Format the output as a markdown. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.",
                    "language1" to "Vietnamese",
                ),
            ),
        ),

        Preset(
            id = "preset_desc",
            nameEn = "Describe image",
            nameVi = "M\u00f4 t\u1ea3 \u1ea3nh",
            nameKo = "\uc774\ubbf8\uc9c0 \uc124\uba85",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                imageBlock(
                    DEFAULT_IMAGE_MODEL_ID,
                    "Describe this image in {language1}. Format the output as a markdown. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.",
                    "language1" to "Vietnamese",
                ),
            ),
        ),

        Preset(
            id = "preset_ask_image",
            nameEn = "Ask about image",
            nameVi = "H\u1ecfi v\u1ec1 \u1ea3nh",
            nameKo = "\uc774\ubbf8\uc9c0 \uc9c8\ubb38",
            presetType = PresetType.IMAGE,
            promptMode = "dynamic",
            blocks = listOf(
                imageBlock(
                    "gemini-3-flash-preview",
                    "",
                    "language1" to "Vietnamese",
                ),
            ),
        ),

        // -- Advanced --------------------------------------------------

        Preset(
            id = "preset_fact_check",
            nameEn = "Fact Check",
            nameVi = "Ki\u1ec3m ch\u1ee9ng th\u00f4ng tin",
            nameKo = "\uc815\ubcf4 \ud655\uc778",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                imageBlock(
                    DEFAULT_IMAGE_MODEL_ID,
                    "Extract and describe all text, claims, statements, and information visible in this image. Include any context that might be relevant for fact-checking. Output the content clearly.",
                    "language1" to "Vietnamese",
                ).copy(showOverlay = false),
                textBlock(
                    "compound_mini",
                    "Fact-check the following claims/information. Search the internet to verify accuracy. Provide a clear verdict (TRUE/FALSE/PARTIALLY TRUE/UNVERIFIABLE) for each claim with evidence and sources. Respond in {language1}. Format as markdown. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.",
                    "language1" to "Vietnamese",
                ),
            ),
        ),

        Preset(
            id = "preset_omniscient_god",
            nameEn = "Omniscient God",
            nameVi = "Th\u1ea7n Tr\u00ed tu\u1ec7",
            nameKo = "\uc804\uc9c0\uc804\ub2a5\ud55c \uc2e0",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                // Node 0: Extract from image
                imageBlock(
                    DEFAULT_IMAGE_MODEL_ID,
                    "Analyze this image and extract all text, claims, and key information. Be detailed and comprehensive.",
                    "language1" to "English",
                ).copy(renderMode = "markdown"),
                // Node 1: Make a learning HTML (from 0)
                textBlock(
                    "cerebras_gpt_oss",
                    "Create a standalone INTERACTIVE HTML learning card/game for the following text. Use internal CSS for a beautiful, modern, colored design, game-like and comprehensive interface. Only OUTPUT the raw HTML code, DO NOT include HTML file indicator (```html) or triple backticks.",
                    "language1" to "Vietnamese",
                ).copy(renderMode = "markdown"),
                // Node 2: Summarize with sources (from 3)
                textBlock(
                    "compound_mini",
                    "Search the internet to ensure of the accuracy of the following text as well as getting as much source information as possible. Summarize the following text into a detailed markdown summary with clickable links to the sources. Structure it clearly. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.",
                    "language1" to "Vietnamese",
                ),
                // Node 3: Translate (from 0)
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate the following text to {language1}. Output ONLY the translation.",
                    "language1" to "Vietnamese",
                ),
                // Node 4: Summarize keywords (from 3)
                textBlock(
                    "cerebras_gpt_oss",
                    "Summarize the essence of this text into 3-5 keywords or a short phrase in {language1}.",
                    "language1" to "Vietnamese",
                ),
            ),
            blockConnections = listOf(0 to 3, 0 to 1, 3 to 4, 3 to 2),
        ),

        Preset(
            id = "preset_hang_image",
            nameEn = "Image Overlay",
            nameVi = "Treo \u1ea3nh",
            nameKo = "\uc774\ubbf8\uc9c0 \uc624\ubc84\ub808\uc774",
            presetType = PresetType.IMAGE,
            blocks = listOf(
                inputAdapter().copy(showOverlay = true, renderMode = "markdown"),
            ),
        ),
    )

    // =====================================================================
    // TEXT SELECT PRESETS (13 presets)
    // =====================================================================

    val textSelectPresets: List<Preset> = listOf(
        Preset(
            id = "preset_read_aloud",
            nameEn = "Read aloud",
            nameVi = "\u0110\u1ecdc to",
            nameKo = "\ud06c\uac8c \uc77d\uae30",
            presetType = PresetType.TEXT_SELECT,
            blocks = listOf(
                inputAdapter().copy(autoSpeak = true),
            ),
        ),

        Preset(
            id = "preset_translate_select",
            nameEn = "Trans (Select text)",
            nameVi = "D\u1ecbch",
            nameKo = "\ubc88\uc5ed (\uc120\ud0dd \ud14d\uc2a4\ud2b8)",
            presetType = PresetType.TEXT_SELECT,
            blocks = listOf(
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate the following text to {language1}. Output ONLY the translation.",
                    "language1" to "Vietnamese",
                ).copy(autoCopy = true),
            ),
        ),

        Preset(
            id = "preset_translate_arena",
            nameEn = "Trans (Arena)",
            nameVi = "D\u1ecbch (Arena)",
            nameKo = "\ubc88\uc5ed (\uc544\ub808\ub098)",
            presetType = PresetType.TEXT_SELECT,
            blocks = listOf(
                // Node 0: Input adapter (text selection)
                inputAdapter(),
                // Node 1: Google Translate (GTX)
                textBlock(
                    "google-gtx",
                    "Translate to {language1}. Output ONLY the translation.",
                    "language1" to "Vietnamese",
                ),
                // Node 2: Cerebras
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate the following text to {language1}. Output ONLY the translation.",
                    "language1" to "Vietnamese",
                ),
                // Node 3: Gemini Flash Lite
                textBlock(
                    "text_gemini_flash_lite",
                    "Translate the following text to {language1}. Output ONLY the translation.",
                    "language1" to "Vietnamese",
                ),
            ),
            blockConnections = listOf(0 to 1, 0 to 2, 0 to 3),
        ),

        Preset(
            id = "preset_trans_retrans_select",
            nameEn = "Trans+Retrans (Select)",
            nameVi = "D\u1ecbch+ D\u1ecbch l\u1ea1i",
            nameKo = "\ubc88\uc5ed+\uc7ac\ubc88\uc5ed (\uc120\ud0dd)",
            presetType = PresetType.TEXT_SELECT,
            blocks = listOf(
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate the following text to {language1}. Output ONLY the translation.",
                    "language1" to "Korean",
                ).copy(autoCopy = true),
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate to {language1}. Output ONLY the translation.",
                    "language1" to "Vietnamese",
                ),
            ),
        ),

        Preset(
            id = "preset_select_translate_replace",
            nameEn = "Select-Trans-Replace",
            nameVi = "D\u1ecbch v\u00e0 Thay",
            nameKo = "\uc120\ud0dd-\ubc88\uc5ed-\uad50\uccb4",
            presetType = PresetType.TEXT_SELECT,
            autoPaste = true,
            blocks = listOf(
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate the following text to {language1}. Output ONLY the translation.",
                    "language1" to "Vietnamese",
                ).copy(
                    renderMode = "markdown",
                    streamingEnabled = false,
                    showOverlay = false,
                    autoCopy = true,
                ),
            ),
        ),

        Preset(
            id = "preset_fix_grammar",
            nameEn = "Fix Grammar",
            nameVi = "S\u1eeda ng\u1eef ph\u00e1p",
            nameKo = "\ubb38\ubc95 \uc218\uc815",
            presetType = PresetType.TEXT_SELECT,
            autoPaste = true,
            blocks = listOf(
                textBlock(
                    "cerebras_gpt_oss",
                    "Correct grammar, spelling, and punctuation errors in the following text. Do not change the meaning or tone. Output ONLY the corrected text.",
                    "language1" to "Vietnamese",
                ).copy(
                    renderMode = "markdown",
                    streamingEnabled = false,
                    showOverlay = false,
                    autoCopy = true,
                ),
            ),
        ),

        Preset(
            id = "preset_rephrase",
            nameEn = "Rephrase",
            nameVi = "Vi\u1ebft l\u1ea1i",
            nameKo = "\ub2e4\uc2dc \uc4f0\uae30",
            presetType = PresetType.TEXT_SELECT,
            autoPaste = true,
            blocks = listOf(
                textBlock(
                    "cerebras_gpt_oss",
                    "Paraphrase the following text using varied vocabulary while maintaining the exact original meaning and language. Output ONLY the paraphrased text.",
                    "language1" to "Vietnamese",
                ).copy(
                    renderMode = "markdown",
                    streamingEnabled = false,
                    showOverlay = false,
                    autoCopy = true,
                ),
            ),
        ),

        Preset(
            id = "preset_make_formal",
            nameEn = "Make Formal",
            nameVi = "Chuy\u00ean nghi\u1ec7p h\u00f3a",
            nameKo = "\uacf5\uc2dd\uc801\uc73c\ub85c",
            presetType = PresetType.TEXT_SELECT,
            autoPaste = true,
            blocks = listOf(
                textBlock(
                    "cerebras_gpt_oss",
                    "Rewrite the following text to be professional and formal, suitable for business communication. CRITICAL: Your output MUST be in the EXACT SAME LANGUAGE as the input text (if input is Korean, output Korean; if Vietnamese, output Vietnamese; if Japanese, output Japanese, etc.). Do NOT translate to English. Maintain the original meaning. Output ONLY the rewritten text.",
                    "language1" to "Vietnamese",
                ).copy(
                    renderMode = "markdown",
                    streamingEnabled = false,
                    showOverlay = false,
                    autoCopy = true,
                ),
            ),
        ),

        Preset(
            id = "preset_explain",
            nameEn = "Explain",
            nameVi = "Gi\u1ea3i th\u00edch",
            nameKo = "\uc124\uba85",
            presetType = PresetType.TEXT_SELECT,
            blocks = listOf(
                textBlock(
                    "cerebras_gpt_oss",
                    "Explain what this is in {language1}. Be concise but thorough. Mention the purpose, key logic, and any important patterns or techniques used. Format the output as a markdown. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) triple backticks.",
                    "language1" to "Vietnamese",
                ),
            ),
        ),

        Preset(
            id = "preset_ask_text",
            nameEn = "Ask about text...",
            nameVi = "H\u1ecfi v\u1ec1 text...",
            nameKo = "\ud14d\uc2a4\ud2b8 \uc9c8\ubb38...",
            presetType = PresetType.TEXT_SELECT,
            promptMode = "dynamic",
            blocks = listOf(
                textBlock(
                    "compound_mini",
                    "",
                    "language1" to "Vietnamese",
                ),
            ),
        ),

        Preset(
            id = "preset_edit_as_follows",
            nameEn = "Edit as follows:",
            nameVi = "S\u1eeda nh\u01b0 sau:",
            nameKo = "\ub2e4\uc74c\uacfc \uac19\uc774 \uc218\uc815:",
            presetType = PresetType.TEXT_SELECT,
            promptMode = "dynamic",
            autoPaste = true,
            blocks = listOf(
                textBlock(
                    "cerebras_gpt_oss",
                    "Edit the following text according to the user's instructions below. Follow the user's request precisely \u2014 if they ask to change the language, change it. Output ONLY the edited result without any introductory text, explanations, or quotes.",
                    "language1" to "Vietnamese",
                ).copy(
                    showOverlay = false,
                    renderMode = "markdown",
                    autoCopy = true,
                ),
            ),
        ),

        Preset(
            id = "preset_101_on_this",
            nameEn = "101 on this",
            nameVi = "T\u1ea5t t\u1ea7n t\u1eadt",
            nameKo = "\uc774\uac83\uc758 \ubaa8\ub4e0 \uac83",
            presetType = PresetType.TEXT_SELECT,
            blocks = listOf(
                // Node 0: Input text
                inputAdapter(),
                // Node 1: Make a learning HTML (from 0)
                textBlock(
                    "cerebras_gpt_oss",
                    "Create a standalone INTERACTIVE HTML learning card/game for the following text. Use internal CSS for a beautiful, modern, colored design, game-like and comprehensive interface. Only OUTPUT the raw HTML code, DO NOT include HTML file indicator (```html) or triple backticks.",
                    "language1" to "Vietnamese",
                ).copy(renderMode = "markdown"),
                // Node 2: Summarize with sources (from 3)
                textBlock(
                    "compound_mini",
                    "Search the internet to ensure of the accuracy of the following text as well as getting as much source information as possible. Summarize the following text into a detailed markdown summary with clickable links to the sources. Structure it clearly. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.",
                    "language1" to "Vietnamese",
                ),
                // Node 3: Translate (from 0)
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate the following text to {language1}. Output ONLY the translation.",
                    "language1" to "Vietnamese",
                ),
                // Node 4: Summarize keywords (from 3)
                textBlock(
                    "cerebras_gpt_oss",
                    "Summarize the essence of this text into 3-5 keywords or a short phrase in {language1}.",
                    "language1" to "Vietnamese",
                ),
            ),
            blockConnections = listOf(0 to 3, 0 to 1, 3 to 4, 3 to 2),
        ),

        Preset(
            id = "preset_hang_text",
            nameEn = "Text Overlay",
            nameVi = "Treo text",
            nameKo = "\ud14d\uc2a4\ud2b8 \uc624\ubc84\ub808\uc774",
            presetType = PresetType.TEXT_SELECT,
            blocks = listOf(
                inputAdapter().copy(showOverlay = true),
            ),
        ),
    )

    // =====================================================================
    // TEXT INPUT (TYPING) PRESETS (5 presets)
    // =====================================================================

    val textInputPresets: List<Preset> = listOf(
        Preset(
            id = "preset_trans_retrans_typing",
            nameEn = "Trans+Retrans (Type)",
            nameVi = "D\u1ecbch+D\u1ecbch l\u1ea1i (T\u1ef1 g\u00f5)",
            nameKo = "\ubc88\uc5ed+\uc7ac\ubc88\uc5ed (\uc785\ub825)",
            presetType = PresetType.TEXT_INPUT,
            continuousInput = true,
            blocks = listOf(
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate the following text to {language1}. Output ONLY the translation. Text to translate:",
                    "language1" to "Korean",
                ).copy(autoCopy = true),
                textBlock(
                    "cerebras_gpt_oss",
                    "Translate to {language1}. Output ONLY the translation.",
                    "language1" to "Vietnamese",
                ),
            ),
        ),

        Preset(
            id = "preset_ask_ai",
            nameEn = "Ask AI",
            nameVi = "H\u1ecfi AI",
            nameKo = "AI \uc9c8\ubb38",
            presetType = PresetType.TEXT_INPUT,
            blocks = listOf(
                textBlock(
                    "cerebras_gpt_oss",
                    "Answer the following question or request helpfully and comprehensively. Format the output as markdown creatively. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks. QUESTION/REQUEST:",
                ),
            ),
        ),

        Preset(
            id = "preset_internet_search",
            nameEn = "Internet Search",
            nameVi = "T\u00ecm ki\u1ebfm internet",
            nameKo = "\uc778\ud130\ub137 \uac80\uc0c9",
            presetType = PresetType.TEXT_INPUT,
            blocks = listOf(
                textBlock(
                    "compound_mini",
                    "Search the internet for information about the following query and provide a comprehensive summary. Include key facts, recent developments, and relevant details with clickable links to sources if possible. Format the output as markdown creatively. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks. SEARCH FOR:",
                ),
            ),
        ),

        Preset(
            id = "preset_make_game",
            nameEn = "Make a Game",
            nameVi = "T\u1ea1o con game",
            nameKo = "\uac8c\uc784 \ub9cc\ub4e4\uae30",
            presetType = PresetType.TEXT_INPUT,
            blocks = listOf(
                textBlock(
                    "text_gemini_3_0_flash",
                    "Create a complete, standalone HTML game. The game MUST be playable using ONLY MOUSE CONTROLS (like swipe , drag or clicks, no keyboard required). Avoid the looping Game Over UI at startup. Use modern and trending CSS on the internet for a polished look, prefer using images or icons or svg assets from the internet for a convincing game aesthetics. Provide HTML code only. Only OUTPUT the raw HTML code, DO NOT include HTML file indicator (```html) or triple backticks. Create the game based on the following request:",
                ).copy(renderMode = "markdown"),
            ),
        ),

        Preset(
            id = "preset_quick_note",
            nameEn = "Quick Note",
            nameVi = "Note nhanh",
            nameKo = "\ube60\ub978 \uba54\ubaa8",
            presetType = PresetType.TEXT_INPUT,
            blocks = listOf(
                inputAdapter().copy(showOverlay = true),
            ),
        ),
    )

    // =====================================================================
    // MIC PRESETS (8 presets)
    // =====================================================================

    val micPresets: List<Preset> = listOf(
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
                    "whisper-accurate",
                    "",
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
                    "gemini-live-audio",
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
                    "whisper-accurate",
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
            blocks = listOf(
                audioBlock(
                    "whisper-accurate",
                    "",
                    "language1" to "Korean",
                ).copy(showOverlay = false),
                textBlock(
                    "cerebras_gpt_oss",
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
            blocks = listOf(
                audioBlock(
                    "gemini-audio",
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
                    "whisper-accurate",
                    "",
                    "language1" to "Vietnamese",
                ).copy(showOverlay = false),
                textBlock(
                    "cerebras_gpt_oss",
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
                    "whisper-accurate",
                    "",
                    "language1" to "Vietnamese",
                ).copy(showOverlay = false),
                textBlock(
                    "compound_mini",
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

    // =====================================================================
    // DEVICE AUDIO PRESETS (4 presets)
    // =====================================================================

    val deviceAudioPresets: List<Preset> = listOf(
        Preset(
            id = "preset_study_language",
            nameEn = "Study language",
            nameVi = "H\u1ecdc ngo\u1ea1i ng\u1eef",
            nameKo = "\uc5b8\uc5b4 \ud559\uc2b5",
            presetType = PresetType.DEVICE_AUDIO,
            audioSource = "device",
            blocks = listOf(
                audioBlock(
                    "whisper-accurate",
                    "",
                    "language1" to "Vietnamese",
                ),
                textBlock(
                    "cerebras_gpt_oss",
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
                audioBlock("whisper-accurate"),
                textBlock(
                    "google-gemma",
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
                    "parakeet-local",
                    "",
                    "language1" to "English",
                ).copy(
                    showOverlay = false,
                    autoCopy = true,
                ),
            ),
        ),
    )

    // =====================================================================
    // MASTER PRESETS (5 presets)
    // =====================================================================

    val masterPresets: List<Preset> = listOf(
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

    // =====================================================================
    // COMBINED
    // =====================================================================

    val all: List<Preset> =
        imagePresets + textSelectPresets + textInputPresets +
            micPresets + deviceAudioPresets + masterPresets
}

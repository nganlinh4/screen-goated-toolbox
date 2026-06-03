package dev.screengoated.toolbox.mobile.shared.preset

/**
 * Text-selection presets (13).
 *
 * Split out of [DefaultPresets] to keep each category file focused. The list is
 * re-exported via [DefaultPresets] so the public API is unchanged. Block helpers
 * (imageBlock/textBlock/audioBlock/inputAdapter) and model-ID constants are
 * package-level declarations in this same package.
 */
internal val defaultTextSelectPresets: List<Preset> = listOf(
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
                DEFAULT_TEXT_MODEL_ID,
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
                PRESET_TRANSLATE_ARENA_GTX_MODEL_ID,
                "Translate to {language1}. Output ONLY the translation.",
                "language1" to "Vietnamese",
            ),
            // Node 2: Cerebras
            textBlock(
                DEFAULT_TEXT_MODEL_ID,
                "Translate the following text to {language1}. Output ONLY the translation.",
                "language1" to "Vietnamese",
            ),
            // Node 3: Gemini Flash Lite
            textBlock(
                PRESET_TEXT_ARENA_FAST_MODEL_ID,
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
                DEFAULT_TEXT_MODEL_ID,
                "Translate the following text to {language1}. Output ONLY the translation.",
                "language1" to "Korean",
            ).copy(autoCopy = true),
            textBlock(
                DEFAULT_TEXT_MODEL_ID,
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
                DEFAULT_TEXT_MODEL_ID,
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
                DEFAULT_TEXT_MODEL_ID,
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
                DEFAULT_TEXT_MODEL_ID,
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
                DEFAULT_TEXT_MODEL_ID,
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
                DEFAULT_TEXT_MODEL_ID,
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
                PRESET_SEARCH_MODEL_ID,
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
                DEFAULT_TEXT_MODEL_ID,
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
                DEFAULT_TEXT_MODEL_ID,
                "Create a standalone INTERACTIVE HTML learning card/game for the following text. Use internal CSS for a beautiful, modern, colored design, game-like and comprehensive interface. Only OUTPUT the raw HTML code, DO NOT include HTML file indicator (```html) or triple backticks.",
                "language1" to "Vietnamese",
            ).copy(renderMode = "markdown"),
            // Node 2: Summarize with sources (from 3)
            textBlock(
                PRESET_SEARCH_MODEL_ID,
                "Search the internet to ensure of the accuracy of the following text as well as getting as much source information as possible. Summarize the following text into a detailed markdown summary with clickable links to the sources. Structure it clearly. Only OUTPUT the markdown, DO NOT include markdown file indicator (```markdown) or triple backticks.",
                "language1" to "Vietnamese",
            ),
            // Node 3: Translate (from 0)
            textBlock(
                DEFAULT_TEXT_MODEL_ID,
                "Translate the following text to {language1}. Output ONLY the translation.",
                "language1" to "Vietnamese",
            ),
            // Node 4: Summarize keywords (from 3)
            textBlock(
                DEFAULT_TEXT_MODEL_ID,
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

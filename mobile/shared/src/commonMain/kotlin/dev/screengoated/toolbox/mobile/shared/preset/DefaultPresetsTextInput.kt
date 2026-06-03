package dev.screengoated.toolbox.mobile.shared.preset

/**
 * Text-input (typing) presets (5).
 *
 * Split out of [DefaultPresets] to keep each category file focused. The list is
 * re-exported via [DefaultPresets] so the public API is unchanged. Block helpers
 * (imageBlock/textBlock/audioBlock/inputAdapter) and model-ID constants are
 * package-level declarations in this same package.
 */
internal val defaultTextInputPresets: List<Preset> = listOf(
    Preset(
        id = "preset_trans_retrans_typing",
        nameEn = "Trans+Retrans (Type)",
        nameVi = "D\u1ecbch+D\u1ecbch l\u1ea1i (T\u1ef1 g\u00f5)",
        nameKo = "\ubc88\uc5ed+\uc7ac\ubc88\uc5ed (\uc785\ub825)",
        presetType = PresetType.TEXT_INPUT,
        continuousInput = true,
        blocks = listOf(
            textBlock(
                DEFAULT_TEXT_MODEL_ID,
                "Translate the following text to {language1}. Output ONLY the translation. Text to translate:",
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
        id = "preset_ask_ai",
        nameEn = "Ask AI",
        nameVi = "H\u1ecfi AI",
        nameKo = "AI \uc9c8\ubb38",
        presetType = PresetType.TEXT_INPUT,
        blocks = listOf(
            textBlock(
                DEFAULT_TEXT_MODEL_ID,
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
                PRESET_SEARCH_MODEL_ID,
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
                PRESET_TEXT_GAME_MODEL_ID,
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

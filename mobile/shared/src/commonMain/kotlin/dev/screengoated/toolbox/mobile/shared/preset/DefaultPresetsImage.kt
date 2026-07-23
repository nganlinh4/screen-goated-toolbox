package dev.screengoated.toolbox.mobile.shared.preset

/**
 * Image presets (16).
 *
 * Split out of [DefaultPresets] to keep each category file focused. The list is
 * re-exported via [DefaultPresets] so the public API is unchanged. Block helpers
 * (imageBlock/textBlock/audioBlock/inputAdapter) and model-ID constants are
 * package-level declarations in this same package.
 */
internal val defaultImagePresets: List<Preset> = listOf(
    // -- Translation -----------------------------------------------

    Preset(
        id = "preset_translate",
        nameEn = "Translate region",
        nameVi = "D\u1ecbch v\u00f9ng",
        nameKo = "\uc601\uc5ed \ubc88\uc5ed",
        presetType = PresetType.IMAGE,
        blocks = listOf(
            imageBlock(
                PRESET_IMAGE_TRANSLATE_VISION_MODEL_ID,
                "Extract text from this image and translate it to {language1}. Output ONLY the translation text directly, do not add introductory text.",
                "language1" to "Vietnamese",
            ).copy(renderMode = "markdown"),
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
                DEFAULT_TEXT_MODEL_ID,
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
                DEFAULT_TEXT_MODEL_ID,
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
                DEFAULT_TEXT_MODEL_ID,
                "Translate to {language1}. Output ONLY the translation.",
                "language1" to "Korean",
            ).copy(autoCopy = true),
            textBlock(
                DEFAULT_TEXT_MODEL_ID,
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
                "qrserver-qr-scanner-vision",
                "",
            ).copy(showOverlay = false, autoCopy = true),
            textBlock(
                DEFAULT_TEXT_MODEL_ID,
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
                PRESET_IMAGE_ASK_MODEL_ID,
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
                PRESET_SEARCH_MODEL_ID,
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

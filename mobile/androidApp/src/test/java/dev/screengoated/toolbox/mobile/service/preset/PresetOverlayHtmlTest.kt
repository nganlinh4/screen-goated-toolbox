package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.inputAdapterOverlayContent
import dev.screengoated.toolbox.mobile.preset.PresetExecutionCapability
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import dev.screengoated.toolbox.mobile.preset.surfaceStreamingEnabled
import dev.screengoated.toolbox.mobile.preset.transportStreamingEnabled
import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.ui.icons.MaterialSymbolSvgs
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertFalse
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetOverlayHtmlTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun favoriteBubbleBuilderKeepsWindowsPanelHooks() {
        val preset = DefaultPresets.all.first { it.id == "preset_ask_ai" }
        val html = FavoriteBubbleHtmlBuilder(materialSymbols()).build(
            FavoriteBubblePanelSettings(
                favorites = listOf(
                    dev.screengoated.toolbox.mobile.preset.ResolvedPreset(
                        preset = preset.copy(isFavorite = true),
                        hasOverride = false,
                        isBuiltIn = true,
                        executionCapability = PresetExecutionCapability(supported = true),
                        placeholderReasons = emptySet(),
                    ),
                ),
                lang = "en",
                isDark = true,
                keepOpenEnabled = false,
                columnCount = 1,
            ),
        )

        assertTrue(html.contains("keep-open-row visible"))
        assertTrue(html.contains("toggleKeepOpen()"))
        assertTrue(html.contains("resizeBubble('desc')"))
        assertTrue(html.contains("resizeBubble('inc')"))
        assertTrue(html.contains("function animateIn"))
        assertTrue(html.contains("function closePanel"))
        assertTrue(html.contains("function showItemsImmediately"))
    }

    @Test
    fun favoriteBubbleBuilderIncludesHangImageWhenFavorited() {
        val preset = DefaultPresets.all.first { it.id == "preset_hang_image" }
        val html = FavoriteBubbleHtmlBuilder(materialSymbols()).build(
            FavoriteBubblePanelSettings(
                favorites = listOf(
                    dev.screengoated.toolbox.mobile.preset.ResolvedPreset(
                        preset = preset.copy(isFavorite = true),
                        hasOverride = false,
                        isBuiltIn = true,
                        executionCapability = PresetExecutionCapability(supported = true),
                        placeholderReasons = emptySet(),
                    ),
                ),
                lang = "en",
                isDark = true,
                keepOpenEnabled = false,
                columnCount = 1,
            ),
        )

        assertTrue(html.contains("Image Overlay"))
        assertFalse(html.contains("""<div class="empty">"""))
    }

    @Test
    fun textInputBuilderKeepsWindowsOverlayHooks() {
        val fixture = fixture("parity-fixtures/preset-system/text-input-overlay.json")
        val canonicalWindowsFiles = fixture["canonical_windows_files"]!!.jsonArray.map { it.jsonPrimitive.content }
        val contract = fixture["android_contract"]!!.jsonObject
        val requiredHooks = contract["required_js_hooks"]!!.jsonArray.map { it.jsonPrimitive.content }
        val acceptedMobileShims = contract["accepted_mobile_shims"]!!.jsonArray.map { it.jsonPrimitive.content }.toSet()
        val html = PresetTextInputHtmlBuilder { name ->
            File(repoRoot(), "mobile/androidApp/build/generated/presetOverlayAssets/preset_overlay/$name").readText()
        }.build(
            PresetTextInputHtmlSettings(
                lang = "en",
                title = "Ask AI",
                placeholder = "Ready...",
                isDark = true,
                owner = "1",
            ),
        )

        canonicalWindowsFiles.forEach { relativePath ->
            assertTrue("missing canonical Windows text-input source $relativePath", File(repoRoot(), relativePath).exists())
        }
        assertTrue(html.contains("""<div class="editor-container">"""))
        requiredHooks.forEach { hook ->
            assertTrue("missing required text-input JS hook $hook", html.contains(hook))
        }
        assertTrue("touch drag shim should be explicitly accepted", "touch_drag_delta_messages" in acceptedMobileShims)
        assertTrue("outside tap focus shim should be explicitly accepted", "outside_tap_focus_release" in acceptedMobileShims)
        assertTrue("drag shim missing", html.contains("""type:'dragInputWindow'"""))
        assertTrue("working microphone bridge message missing", html.contains("postInput('mic')"))
        assertFalse("template tokens must be resolved", Regex("\\{\\{[A-Z_]+}}").containsMatchIn(html))
    }

    @Test
    fun resultSupportUsesMarkdownOnlyOverlayHooks() {
        val fixture = fixture("parity-fixtures/preset-system/result-overlay.json")
        val markdownView = fixture["markdown_view"]!!.jsonObject
        val canvas = fixture["canvas"]!!.jsonObject
        val html = presetResultBaseHtmlTemplate()
        val js = presetResultJavascript()

        assertEquals("markdown_only", fixture["render_mode"]!!.jsonPrimitive.content)
        assertEquals("precreate_before_result_text", fixture["window_creation"]!!.jsonPrimitive.content)
        assertEquals("body_level_markdown_content", markdownView["dom_contract"]!!.jsonPrimitive.content)
        assertEquals("markdown_plus_raw_html", markdownView["render_mode"]!!.jsonPrimitive.content)
        assertTrue(markdownView["raw_html_supported"]!!.jsonPrimitive.boolean)
        assertEquals(
            "loaded_document_is_never_replaced_by_generic_body_inner_html",
            markdownView["raw_html_document_authority"]!!.jsonPrimitive.content,
        )
        assertEquals(
            "inline_scripts_execute_once_during_document_load_and_keep_live_state",
            markdownView["raw_html_script_lifecycle"]!!.jsonPrimitive.content,
        )
        assertTrue(markdownView["gridjs_enabled_for_tables"]!!.jsonPrimitive.boolean)
        assertEquals("shared_pinned_windows_bundle", markdownView["gridjs_assets"]!!.jsonPrimitive.content)
        assertEquals("windows_result_compositor_allowlist", markdownView["markdown_html_policy"]!!.jsonPrimitive.content)
        assertEquals("user_gesture_required", markdownView["android_external_intent_policy"]!!.jsonPrimitive.content)
        assertEquals(
            "separate_raw_and_intercepted_navigation_webview_other_external_documents_origin_denied",
            markdownView["android_untrusted_document_policy"]!!.jsonPrimitive.content,
        )
        assertEquals("origin_bound_main_frame_only", markdownView["android_trusted_result_bridge"]!!.jsonPrimitive.content)
        assertEquals(OverlayNavigationPolicy.MANAGED, overlayNavigationPolicy("https://example.com", false))
        assertEquals(OverlayNavigationPolicy.BLOCK, overlayNavigationPolicy("sms:123", false))
        assertEquals(OverlayNavigationPolicy.EXTERNAL_INTENT, overlayNavigationPolicy("sms:123", true))
        assertEquals(2000, canvas["linger_ms"]!!.jsonPrimitive.int)
        assertTrue(html.contains("{{FIT_SCRIPT}}"))
        assertTrue(html.contains("{{THEME_CSS}}"))
        assertTrue(html.contains("{{GRIDJS_LIB_CSS}}"))
        assertTrue(html.contains("{{GRIDJS_LIB_JS}}"))
        assertTrue(html.contains("{{RESULT_DOM_PATCH_JS}}"))
        assertTrue(html.contains("{{M3E_LIB_JS}}"))
        assertTrue(html.contains("{{M3E_INIT_JS}}"))
        assertTrue(js.contains("window._sgtLoadingAnimation.stop()"))
        assertTrue(js.contains("window.sgtStartLoading(canvas)"))
        assertTrue(js.contains("document.body.replaceChildren(window.__SGT_SAFE_FRAGMENT__(html || ''))"))
        assertTrue(html.contains("{{GRIDJS_INIT_SCRIPT}}"))
        assertTrue(html.contains("""<body></body>"""))
        assertTrue(js.contains("applyStreamingResultState"))
        assertTrue(js.contains("applyFinalResultState"))
        assertTrue(js.contains("window.applyResultState"))
        assertTrue(js.contains("if (data.loading)"))
        assertTrue(js.contains("applyLoadingHtml(data.html)"))
        assertTrue(js.contains("window._streamWordCount = newWordCount"))
        assertTrue(js.contains("window._streamRenderCount = prevRenderCount + 1"))
        assertTrue(js.contains("wrapInteractiveWords(document.body)"))
        assertTrue(js.contains("event.touches.length > 1"))
        assertTrue(js.contains("""type: 'dragResultWindow'"""))
        assertTrue(js.contains("""type: 'dragResultWindowAt'"""))
        assertTrue(js.contains("""type: 'dragResultWindowEnd'"""))
        assertTrue(js.contains("""type: 'resizeResultWindow'"""))
        assertTrue(js.contains("""type: 'resizeResultWindowEnd'"""))
        assertTrue(js.contains("selection_mode_begin"))
        assertTrue(js.contains("""type: 'copySelectedText'"""))
        assertTrue(js.contains("scheduleCustomSelection"))
        assertTrue(js.contains("updateCustomSelection"))
        assertTrue(js.contains("selectionHandleElement"))
        assertTrue(js.contains("updateSelectionHandles"))
        assertTrue(js.contains("edgeCaretRect"))
        assertTrue(js.contains("scheduleHandleUpdate"))
        assertTrue(js.contains("touchstart_selection_handle"))
        assertTrue(js.contains("document.addEventListener('dragstart'"))
        assertTrue(js.contains("window.ipc.postMessage('result_ready')"))
        assertFalse(js.contains("plainText"))
    }

    @Test
    fun makeGameAllowsGameAppropriateInput() {
        val fixture = fixture("parity-fixtures/preset-system/result-overlay.json")
        val contract = fixture["interactive_html"]!!.jsonObject
        val preset = DefaultPresets.all.first { it.id == "preset_make_game" }
        val prompt = preset.blocks.first().prompt

        assertEquals("preset_make_game", contract["preset_id"]!!.jsonPrimitive.content)
        assertEquals(
            "game_appropriate_keyboard_pointer_touch_or_combination",
            contract["input_policy"]!!.jsonPrimitive.content,
        )
        assertTrue(contract["visible_control_instructions"]!!.jsonPrimitive.boolean)
        assertFalse(contract["mouse_only_constraint"]!!.jsonPrimitive.boolean)
        assertTrue(prompt.contains("keyboard, mouse/pointer, touch, or a combination"))
        assertTrue(prompt.contains("after the player clicks or taps the game"))
        assertFalse(prompt.contains("ONLY MOUSE CONTROLS"))
        assertFalse(prompt.contains("no keyboard required"))
        val model = requireNotNull(
            dev.screengoated.toolbox.mobile.preset.PresetModelCatalog.getById(
                preset.blocks.first().model,
            ),
        )
        assertTrue(model.transportStreamingEnabled())
        assertFalse(preset.blocks.first().surfaceStreamingEnabled())
    }

    @Test
    fun rawHtmlInteractionBridgeKeepsOverlayControls() {
        val fixture = fixture("parity-fixtures/preset-system/result-overlay.json")
        val markdownView = fixture["markdown_view"]!!.jsonObject
        val js = presetResultInteractionJavascript()

        assertEquals("isolated_world_overlay_shell_when_supported", markdownView["raw_html_host_contract"]!!.jsonPrimitive.content)
        assertEquals("native_history_plus_restore_original_surface", markdownView["navigation_model"]!!.jsonPrimitive.content)
        assertEquals("restore_original_surface", markdownView["failed_external_navigation_policy"]!!.jsonPrimitive.content)
        assertTrue(js.contains("window.configureResultWindow"))
        assertTrue(js.contains("event.touches.length > 1"))
        assertTrue(js.contains("elementCanScrollAxis"))
        assertTrue(js.contains("""type: 'dragResultWindow'"""))
        assertTrue(js.contains("""type: 'resizeResultWindow'"""))
        assertTrue(js.contains("""type: 'copySelectedText'"""))
        assertTrue(js.contains("selection_mode_begin"))
        assertTrue(js.contains("selectionHandleElement"))
        assertTrue(js.contains("updateSelectionHandles"))
        assertTrue(js.contains("edgeCaretRect"))
        assertTrue(js.contains("scheduleHandleUpdate"))
        assertTrue(js.contains("window.ipc.postMessage('result_ready')"))
    }

    @Test
    fun hostedRawHtmlBootstrapReappliesOverlayShell() {
        val fixture = fixture("parity-fixtures/preset-system/result-overlay.json")
        val markdownView = fixture["markdown_view"]!!.jsonObject
        val script = presetHostedRawPageBootstrapScript(
            windowId = "result:test",
            isDark = true,
        )

        assertEquals("isolated_world_overlay_shell_when_supported", markdownView["raw_html_host_contract"]!!.jsonPrimitive.content)
        assertTrue(script.contains("__SGT_RESULT_INTERACTION_INSTALLED__"))
        assertTrue(script.contains("sgt-result-hosted-page-style"))
        assertTrue(script.contains("""window.configureResultWindow("result:test")"""))
        assertTrue(script.contains("scrollbar-width: none"))
        assertFalse(script.contains("backdrop-filter: blur(18px)"))
    }

    @Test
    fun rawHtmlNormalizationRemovesOnlyTransportDecoration() {
        val fenced = """
            ```html
            <!doctype html><html><body><button>Play</button><script>window.ready=true</script></body></html>
            ```
        """.trimIndent()
        val normalized = requireNotNull(normalizePresetHtmlContent(fenced))

        assertTrue(normalized.startsWith("<!doctype html>"))
        assertTrue(normalized.contains("window.ready=true"))
        assertFalse(normalized.contains("```"))
        assertEquals(null, normalizePresetHtmlContent("Use `<button>` in Markdown."))
        assertEquals(null, normalizePresetHtmlContent("İ <HTML><body>hello</body></HTML>"))
    }

    @Test
    fun rawHtmlClassificationMatchesSharedCases() {
        val cases = fixture("parity-fixtures/preset-system/result-overlay.json")
            .getValue("markdown_view").jsonObject
            .getValue("raw_html_classification_cases").jsonArray
        for (case in cases) {
            val item = case.jsonObject
            assertEquals(
                item.getValue("name").jsonPrimitive.content,
                item.getValue("raw").jsonPrimitive.boolean,
                normalizePresetHtmlContent(item.getValue("input").jsonPrimitive.content) != null,
            )
        }
    }

    @Test
    fun deeplyNestedForeignContentIsRejected() {
        val input = "<html>" + "<svg>".repeat(65) + "</svg>".repeat(65) + "</html>"
        assertEquals(null, normalizePresetHtmlContent(input))
    }

    @Test
    fun rawHtmlContextDepthIsBounded() {
        for ((depth, raw) in listOf(64 to true, 65 to false)) {
            val document = "<html>" + "<template>".repeat(depth) + "</template>".repeat(depth) + "</html>"
            assertEquals(raw, normalizePresetHtmlContent(document) != null)
        }
    }

    @Test
    fun hostedRawHtmlCssDoesNotOverrideTheGeneratedExperience() {
        val css = presetHostedRawPageCss(isDark = true)

        assertTrue(css.contains(".sgt-selection-action"))
        assertFalse(css.contains("backdrop-filter: blur(18px)"))
        assertFalse(css.contains("body[data-sgt-result-hosted='1'] {\n    border:"))
        assertFalse(css.contains("body *"))
        assertFalse(css.contains("touch-action: manipulation !important"))
    }

    @Test
    fun hostedRawHtmlBootstrapUsesMinimalBodyChromeForInputAdapterMedia() {
        val script = presetHostedRawPageBootstrapScript(
            windowId = "result:test",
            isDark = true,
            isInputAdapterMedia = true,
        )
        val css = presetHostedRawPageCss(isDark = true, isInputAdapterMedia = true)

        assertTrue(script.contains("data-sgt-input-adapter-media-hosted"))
        assertTrue(css.contains("background: transparent !important;"))
        assertTrue(css.contains("border: none !important;"))
        assertTrue(css.contains("box-shadow: none !important;"))
    }

    @Test
    fun inputAdapterMediaHtmlKeepsWindowsMediaMarkers() {
        val fixture = fixture("parity-fixtures/preset-system/result-overlay.json")
        val mediaContract = fixture["markdown_view"]!!.jsonObject["input_adapter_media_contract"]!!.jsonObject
        val preserveMarkers = mediaContract["preserve_markers"]!!.jsonArray.map { it.jsonPrimitive.content }
        val imageHtml = inputAdapterOverlayContent(
            PresetInput.Image(byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47)),
            "en",
        ).orEmpty()
        val audioHtml = inputAdapterOverlayContent(
            PresetInput.Audio(
                byteArrayOf(
                    'R'.code.toByte(),
                    'I'.code.toByte(),
                    'F'.code.toByte(),
                    'F'.code.toByte(),
                    0, 0, 0, 0,
                    'W'.code.toByte(),
                    'A'.code.toByte(),
                    'V'.code.toByte(),
                    'E'.code.toByte(),
                ),
            ),
            "en",
        ).orEmpty()

        preserveMarkers.forEach { marker ->
            assertTrue("missing media marker $marker", imageHtml.contains(marker) || audioHtml.contains(marker))
        }
        assertTrue(imageHtml.contains("""data-sgt-input-adapter-media="image""""))
        assertTrue(imageHtml.contains("""class="container""""))
        assertTrue(audioHtml.contains("""data-sgt-input-adapter-media="audio""""))
        assertTrue(audioHtml.contains("""class="audio-player""""))
        assertTrue(audioHtml.contains("""class="waveform""""))
        assertFalse(audioHtml.contains("saveMediaToDownloads"))
        assertEquals("native_result_bytes_and_trusted_user_action", mediaContract["android_audio_download"]!!.jsonPrimitive.content)
        assertEquals("reports_failure_and_native_control_available", mediaContract["android_audio_download_without_isolated_gesture"]!!.jsonPrimitive.content)
        assertTrue(audioHtml.contains("data-sgt-native-audio-download"))
        assertTrue(audioHtml.contains("toast.textContent = 'Could not download'"))
    }

    @Test
    fun audioInputAdapterUsesAccentedVietnameseDownloadLabels() {
        val audioHtml = inputAdapterOverlayContent(
            PresetInput.Audio(
                byteArrayOf(
                    'R'.code.toByte(),
                    'I'.code.toByte(),
                    'F'.code.toByte(),
                    'F'.code.toByte(),
                    0, 0, 0, 0,
                    'W'.code.toByte(),
                    'A'.code.toByte(),
                    'V'.code.toByte(),
                    'E'.code.toByte(),
                ),
            ),
            "vi",
        ).orEmpty()

        assertTrue(audioHtml.contains("Đã tải xuống"))
        assertTrue(audioHtml.contains("Tải xuống"))
        assertTrue(audioHtml.contains("Không thể tải xuống"))
    }

    @Test
    fun rawHtmlInteractionPreventsSingleTouchScrollBeforeDragWins() {
        val js = presetResultInteractionJavascript()

        assertTrue(js.contains("pendingStart && !selectionHandleDrag && !selectionGestureActive"))
        assertTrue(js.contains("event.preventDefault()"))
    }

    @Test
    fun buttonCanvasSupportUsesSharedTouchRevealHooks() {
        val fixture = fixture("parity-fixtures/preset-system/result-overlay.json")
        val canvas = fixture["canvas"]!!.jsonObject
        val unsupportedActions = canvas["unsupported_actions"]!!.jsonArray.map { it.jsonPrimitive.content }.toSet()
        val modelBadge = canvas["model_badge"]!!.jsonObject
        val html = presetButtonCanvasBaseHtmlTemplate()
        val js = mobileCanvasJavascript()

        assertEquals("tap_and_linger", canvas["reveal_model"]!!.jsonPrimitive.content)
        assertEquals(2000, canvas["linger_ms"]!!.jsonPrimitive.int)
        assertEquals(
            "actual_successful_retry_endpoint",
            modelBadge["identity"]!!.jsonPrimitive.content,
        )
        assertFalse("markdown toggle is supported", "markdown_toggle" in unsupportedActions)
        assertTrue("broom group/all exclusion should stay documented", "broom_group_all" in unsupportedActions)
        assertTrue(html.contains("""<div id="button-container"></div>"""))
        assertTrue(js.contains("window.setCanvasWindows"))
        assertTrue(js.contains("window.revealWindow"))
        assertTrue(js.contains("placeholder_action"))
        assertTrue(js.contains("update_clickable_regions"))
        assertTrue(js.contains("""querySelector('[data-action="markdown"]')"""))
        assertTrue(js.contains("""querySelector('.btn.broom')"""))
        assertFalse(js.contains("plainTextLabel"))
    }

    @Test
    fun resultBadgeUsesProviderAndCompleteSuccessfulEndpoint() {
        val state = PresetResultWindowState(
            id = PresetResultWindowId("session", 1),
            blockIdx = 1,
            title = "Result",
            modelId = "nvidia-nemotron-3-5-lightning-text",
            modelProvider = PresetModelProvider.NVIDIA,
        )

        assertEquals(
            "NVIDIA · nvidia/nemotron-3.5-lightning-30b-a3b",
            resultModelLabel(state),
        )
        assertEquals("", resultModelLabel(state.copy(modelId = "", modelProvider = null)))
    }

    private fun materialSymbols() = MaterialSymbolSvgs { fileName ->
        File(repoRoot(), "mobile/androidApp/src/main/assets/material-symbols/$fileName").readText()
    }

    private fun fixture(path: String) =
        json.parseToJsonElement(File(repoRoot(), path).readText()).jsonObject

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile
        }.first { root ->
            File(root, "parity-fixtures/preset-system/result-overlay.json").exists()
        }
    }
}

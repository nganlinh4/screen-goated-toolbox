package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.inputAdapterOverlayContent
import dev.screengoated.toolbox.mobile.preset.PresetExecutionCapability
import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
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
        val html = FavoriteBubbleHtmlBuilder().build(
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
        val html = FavoriteBubbleHtmlBuilder().build(
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
        val contract = fixture["android_contract"]!!.jsonObject
        val requiredMessages = contract["required_messages"]!!.jsonArray.map { it.jsonPrimitive.content }
        val requiredHooks = contract["required_js_hooks"]!!.jsonArray.map { it.jsonPrimitive.content }
        val acceptedMobileShims = contract["accepted_mobile_shims"]!!.jsonArray.map { it.jsonPrimitive.content }.toSet()
        val documentedDeferred = fixture["documented_deferred_behavior"]!!.jsonArray.map { it.jsonPrimitive.content }
        val html = PresetTextInputHtmlBuilder().build(
            PresetTextInputHtmlSettings(
                lang = "en",
                title = "Ask AI",
                placeholder = "Ready...",
                isDark = true,
            ),
        )

        assertTrue(html.contains("""<div class="editor-container">"""))
        requiredMessages.forEach { message ->
            assertTrue("missing required text-input message $message", html.contains(messageContractSnippet(message)))
        }
        requiredHooks.forEach { hook ->
            assertTrue("missing required text-input JS hook $hook", html.contains(hook))
        }
        assertTrue("touch drag shim should be explicitly accepted", "touch_drag_delta_messages" in acceptedMobileShims)
        assertTrue("outside tap focus shim should be explicitly accepted", "outside_tap_focus_release" in acceptedMobileShims)
        assertTrue("drag shim missing", html.contains("""type: 'dragInputWindow'"""))
        assertTrue("deferred mic runtime should keep the bridge message visible", "mic_button_runtime" in documentedDeferred)
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
        assertTrue(markdownView["gridjs_enabled_for_tables"]!!.jsonPrimitive.boolean)
        assertEquals(2000, canvas["linger_ms"]!!.jsonPrimitive.int)
        assertTrue(html.contains("{{FIT_SCRIPT}}"))
        assertTrue(html.contains("{{THEME_CSS}}"))
        assertTrue(html.contains("{{GRIDJS_CSS_URL}}"))
        assertTrue(html.contains("{{GRIDJS_INIT_SCRIPT}}"))
        assertTrue(html.contains("""<body></body>"""))
        assertTrue(js.contains("applyStreamingResultState"))
        assertTrue(js.contains("applyFinalResultState"))
        assertTrue(js.contains("window.applyResultState"))
        assertTrue(js.contains("if (data.loading)"))
        assertTrue(js.contains("document.body.innerHTML = data.html || ''"))
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
    fun rawHtmlInteractionBridgeKeepsOverlayControls() {
        val fixture = fixture("parity-fixtures/preset-system/result-overlay.json")
        val markdownView = fixture["markdown_view"]!!.jsonObject
        val js = presetResultInteractionJavascript()

        assertEquals("page_load_reinjected_overlay_shell", markdownView["raw_html_host_contract"]!!.jsonPrimitive.content)
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

        assertEquals("page_load_reinjected_overlay_shell", markdownView["raw_html_host_contract"]!!.jsonPrimitive.content)
        assertTrue(script.contains("__SGT_RESULT_INTERACTION_INSTALLED__"))
        assertTrue(script.contains("sgt-result-hosted-page-style"))
        assertTrue(script.contains("""window.configureResultWindow("result:test")"""))
        assertTrue(script.contains("overflow-y: auto;"))
        assertTrue(script.contains("overflow-x: auto;"))
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
        assertTrue(audioHtml.contains("saveMediaToDownloads"))
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
        val html = presetButtonCanvasBaseHtmlTemplate()
        val js = mobileCanvasJavascript()

        assertEquals("tap_and_linger", canvas["reveal_model"]!!.jsonPrimitive.content)
        assertEquals(2000, canvas["linger_ms"]!!.jsonPrimitive.int)
        assertTrue("markdown toggle exclusion should stay documented", "markdown_toggle" in unsupportedActions)
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

    private fun messageContractSnippet(message: String): String = when (message) {
        "submit:<text>" -> "window.ipc.postMessage('submit:' + text)"
        "cancel" -> "window.ipc.postMessage('cancel')"
        "close_window" -> "window.ipc.postMessage('close_window')"
        "history_up:<draft>" -> "window.ipc.postMessage('history_up:' + editor.value)"
        "history_down:<draft>" -> "window.ipc.postMessage('history_down:' + editor.value)"
        "mic" -> "window.ipc.postMessage('mic')"
        "request_focus" -> "window.ipc.postMessage('request_focus')"
        else -> error("Unsupported text-input fixture message: $message")
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

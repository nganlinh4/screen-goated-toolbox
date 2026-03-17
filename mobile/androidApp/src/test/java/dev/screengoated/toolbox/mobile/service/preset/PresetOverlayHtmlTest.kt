package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetExecutionCapability
import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetOverlayHtmlTest {
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
    fun textInputBuilderKeepsWindowsOverlayHooks() {
        val html = PresetTextInputHtmlBuilder().build(
            PresetTextInputHtmlSettings(
                lang = "en",
                title = "Ask AI",
                placeholder = "Ready...",
                isDark = true,
            ),
        )

        assertTrue(html.contains("""<div class="editor-container">"""))
        assertTrue(html.contains("window.ipc.postMessage('submit:' + text)"))
        assertTrue(html.contains("window.ipc.postMessage('history_up:' + editor.value)"))
        assertTrue(html.contains("window.ipc.postMessage('history_down:' + editor.value)"))
        assertTrue(html.contains("window.exportDraftState"))
        assertTrue(html.contains("window.restoreDraftState"))
        assertTrue(html.contains("""type: 'dragInputWindow'"""))
    }

    @Test
    fun resultSupportUsesMarkdownOnlyOverlayHooks() {
        val html = presetResultBaseHtmlTemplate()
        val js = presetResultJavascript()

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
        val js = presetResultInteractionJavascript()

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
        val script = presetHostedRawPageBootstrapScript(
            windowId = "result:test",
            isDark = true,
        )

        assertTrue(script.contains("__SGT_RESULT_INTERACTION_INSTALLED__"))
        assertTrue(script.contains("sgt-result-hosted-page-style"))
        assertTrue(script.contains("""window.configureResultWindow("result:test")"""))
        assertTrue(script.contains("overflow-y: auto;"))
        assertTrue(script.contains("overflow-x: auto;"))
    }

    @Test
    fun rawHtmlInteractionPreventsSingleTouchScrollBeforeDragWins() {
        val js = presetResultInteractionJavascript()

        assertTrue(js.contains("pendingStart && !selectionHandleDrag && !selectionGestureActive"))
        assertTrue(js.contains("event.preventDefault()"))
    }

    @Test
    fun buttonCanvasSupportUsesSharedTouchRevealHooks() {
        val html = presetButtonCanvasBaseHtmlTemplate()
        val js = mobileCanvasJavascript()

        assertTrue(html.contains("""<div id="button-container"></div>"""))
        assertTrue(js.contains("window.setCanvasWindows"))
        assertTrue(js.contains("window.revealWindow"))
        assertTrue(js.contains("placeholder_action"))
        assertTrue(js.contains("update_clickable_regions"))
        assertTrue(js.contains("""querySelector('[data-action="markdown"]')"""))
        assertTrue(js.contains("""querySelector('.btn.broom')"""))
        assertFalse(js.contains("plainTextLabel"))
    }
}

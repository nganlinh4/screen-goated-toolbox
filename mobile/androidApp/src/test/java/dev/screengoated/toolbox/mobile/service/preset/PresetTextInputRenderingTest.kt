package dev.screengoated.toolbox.mobile.service.preset

import java.io.File
import org.junit.Assert.*
import org.junit.Test

class PresetTextInputRenderingTest {
    private fun builder(): PresetTextInputHtmlBuilder {
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .first { File(it,"parity-fixtures/preset-system/text-input-overlay.json").isFile }
        return PresetTextInputHtmlBuilder { name ->
            File(root,"mobile/androidApp/build/generated/presetOverlayAssets/preset_overlay/$name").readText()
        }
    }

    @Test
    fun authoredTemplateTokensRemainLiteralAndMarkupIsEscapedInBothThemes() {
        for (dark in listOf(false,true)) {
            val html = builder().build(PresetTextInputHtmlSettings("en","{{CSS}} <title>","\" <input>",dark,"owner"))
            assertTrue(html.contains("{{CSS}} &lt;title&gt;"))
            assertTrue(html.contains("&quot; &lt;input&gt;"))
            assertFalse(html.contains("<input>"))
            assertTrue(html.contains("id=\"footerRegion\""))
            assertTrue(html.contains("data-theme=\"${if (dark) "dark" else "light"}\""))
        }
    }

    @Test
    fun preferenceProjectionKeepsOwnerAndEscapesAuthoredJavascriptText() {
        val script = builder().chromeScript(PresetTextInputHtmlSettings("en","title\";throw 1;//","placeholder",false,"owner"))
        assertTrue(script.contains("window.__sgtTextSession === \"owner\""))
        assertTrue(script.contains("title\\\";throw 1;//"))
        assertTrue(script.contains("Enter to Submit"))
        assertTrue(script.contains("--bg-color: rgba(255, 255, 255, 0.75)"))
    }
}

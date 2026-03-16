package dev.screengoated.toolbox.mobile.service.preset

import org.commonmark.parser.Parser
import org.commonmark.renderer.html.HtmlRenderer

internal class PresetMarkdownRenderer {
    private val parser = Parser.builder().build()
    private val renderer = HtmlRenderer.builder().escapeHtml(false).build()

    fun render(markdown: String): String {
        val document = parser.parse(markdown)
        return renderer.render(document)
    }
}

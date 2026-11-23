package dev.screengoated.toolbox.mobile.service.preset

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetRawDocumentLoadTest {
    private val doc = "<!DOCTYPE html><html><body>game</body></html>"

    @Test
    fun documentCompletedMidStreamWaitsForTheStreamToEnd() {
        assertEquals(RawDocumentLoad.DEFER, rawDocumentLoad(isStreaming = true, loadedRawHtml = null, html = doc))
        assertEquals(RawDocumentLoad.LOAD, rawDocumentLoad(isStreaming = false, loadedRawHtml = null, html = doc))
    }

    @Test
    fun identicalDocumentIsNeverReloaded() {
        assertEquals(RawDocumentLoad.KEEP, rawDocumentLoad(isStreaming = false, loadedRawHtml = doc, html = doc))
        assertEquals(RawDocumentLoad.KEEP, rawDocumentLoad(isStreaming = true, loadedRawHtml = doc, html = doc))
    }

    @Test
    fun changedDocumentLoadsOnceTheStreamEnds() {
        val refined = doc.replace("game", "refined game")
        assertEquals(RawDocumentLoad.DEFER, rawDocumentLoad(isStreaming = true, loadedRawHtml = doc, html = refined))
        assertEquals(RawDocumentLoad.LOAD, rawDocumentLoad(isStreaming = false, loadedRawHtml = doc, html = refined))
    }

    @Test
    fun streamingIsNotGatedByPromptKeywords() {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) directory = requireNotNull(directory.parentFile)
        val main = File(directory, "mobile/androidApp/src/main/java")
        val offenders = main.walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            .filter { it.readText().contains("requestsHtmlOutput") }
            .map { it.name }
            .toList()
        assertEquals(emptyList<String>(), offenders)
        val lifecycle = File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/PresetOverlayResultLifecycleSupport.kt",
        ).readText()
        assertTrue(lifecycle.contains("rawDocumentLoad("))
    }
}

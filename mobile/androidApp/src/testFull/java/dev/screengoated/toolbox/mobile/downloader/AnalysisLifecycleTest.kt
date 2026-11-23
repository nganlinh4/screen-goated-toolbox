package dev.screengoated.toolbox.mobile.downloader

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.atomic.AtomicBoolean
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class AnalysisLifecycleTest {
    @Test
    fun staleGenerationAndClosedTabCannotPublish() {
        var root = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(root, "parity-fixtures/video-downloader/analysis-lifecycle.json").isFile) {
            root = root.parentFile ?: error("fixture missing")
        }
        val fixture = JSONObject(File(root, "parity-fixtures/video-downloader/analysis-lifecycle.json").readText())
        assertEquals(2, fixture.getInt("maxConcurrentScans"))
        val old = AnalysisControl()
        val replacement = DownloadSessionState(id = 2, tabName = "Tab", inputUrl = "a", analysisGeneration = 3)
        val actual = applyAnalysisIfCurrent(replacement, 2, 1, "a", old) { it.copy(availableFormats = listOf("old")) }
        assertTrue(actual.availableFormats.isEmpty())
        assertEquals(replacement, applyAnalysisIfCurrent(replacement, 1, 3, "a", old) { it.copy(availableFormats = listOf("wrong tab")) })
        val newControl = AnalysisControl()
        val ready = applyAnalysisIfCurrent(replacement, 2, 3, "a", newControl) { it.copy(availableFormats = listOf("new")) }
        assertEquals(listOf("new"), ready.availableFormats)
        newControl.retire()
        assertEquals(ready, applyAnalysisIfCurrent(ready, 2, 3, "a", newControl) { it.copy(analysisError = "old") })
    }

    @Test
    fun cancellationCoversPreRegistrationAndLeavesOtherChildrenUntouched() {
        val retired = AnalysisControl()
        retired.retire()
        val late = FakeProcess()
        assertFalse(retired.attach(late))
        assertTrue(late.destroyed)
        val current = AnalysisControl()
        val owned = FakeProcess()
        val unrelated = FakeProcess()
        assertTrue(current.attach(owned))
        current.retire()
        current.stopOwned()
        assertTrue(owned.destroyed)
        assertFalse(unrelated.destroyed)
    }

    @Test
    fun tabKeepsExplicitBestWhenAnotherTabChangesRememberedQuality() {
        val bestTab = newDownloadSession(1, "best", DownloadType.VIDEO, null)
        val otherTab = newDownloadSession(2, "other", DownloadType.VIDEO, null).copy(selectedFormat = "720p")
        val newTab = newDownloadSession(3, "new", DownloadType.VIDEO, otherTab.selectedFormat)
        assertNull(bestTab.selectedFormat)
        assertNull(resolutionSort(bestTab.selectedFormat))
        assertEquals("720p", otherTab.selectedFormat)
        assertEquals("res:720", resolutionSort(otherTab.selectedFormat))
        assertEquals("720p", newTab.selectedFormat)
    }

    @Test
    fun analysisOutputIsBoundedWithoutLaunchingAProcess() {
        val output = StringBuffer()
        val overflow = AtomicBoolean(false)
        val reader = readAnalysisStream(ByteArrayInputStream("abcd".toByteArray()), output, 3, overflow)
        reader.join()
        assertTrue(overflow.get())
        assertEquals("abc", output.toString())
    }

    private class FakeProcess : Process() {
        var destroyed = false
        override fun getOutputStream(): OutputStream = ByteArrayOutputStream()
        override fun getInputStream(): InputStream = ByteArrayInputStream(byteArrayOf())
        override fun getErrorStream(): InputStream = ByteArrayInputStream(byteArrayOf())
        override fun waitFor(): Int = 0
        override fun exitValue(): Int = 0
        override fun destroy() { destroyed = true }
        override fun destroyForcibly(): Process { destroyed = true; return this }
    }
}

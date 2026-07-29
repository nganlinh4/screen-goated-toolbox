package dev.screengoated.toolbox.mobile.creation

import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class CreationSvgBackgroundModeTest {
    @Test
    fun `background mode normalizes to the compatibility default`() {
        assertEquals("opaque", normalizeSvgBackgroundMode(null))
        assertEquals("opaque", normalizeSvgBackgroundMode("unknown"))
        assertEquals("auto", normalizeSvgBackgroundMode("auto"))
        assertEquals("transparent", normalizeSvgBackgroundMode("transparent"))
    }

    @Test
    fun `submission carries the selected background mode`() {
        val args = creationSubmissionArgs(
            CreationTool.IMAGE_TO_SVG,
            CreationNativeItem(
                id = "item",
                batchId = "batch",
                sourcePath = "source.png",
                sourceName = "source.png",
                backgroundMode = "transparent",
            ),
        )
        assertEquals("transparent", args.getValue("backgroundMode").jsonPrimitive.content)
    }

    @Test
    fun `background choice participates in delivery identity`() {
        val base = CreationWorkerRequest(
            jobId = "job",
            dispatchId = "dispatch",
            requestFingerprint = "",
            tool = "svg",
            operation = "generate",
            imagePath = "source.png",
            outputPath = "result.svg",
            outputName = "result.svg",
        )
        assertNotEquals(
            creationRequestFingerprint(base.copy(backgroundMode = "opaque")),
            creationRequestFingerprint(base.copy(backgroundMode = "transparent")),
        )
    }
}

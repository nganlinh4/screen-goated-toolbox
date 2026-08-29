package dev.screengoated.toolbox.mobile.preset

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class InitialLineBreakNormalizerTest {
    @Test
    fun removesOnlyInitialLineBreaksAcrossStreamChunks() {
        val normalizer = InitialLineBreakNormalizer()
        assertNull(normalizer.observe("\r"))
        assertNull(normalizer.observe("\n\n"))
        assertEquals("  result", normalizer.observe("  result"))
        assertEquals("\nnext", normalizer.observe("\nnext"))
        assertEquals("  result\nnext", normalizer.finish("\r\n  result\nnext"))
    }

    @Test
    fun replacementRestartsInitialNormalization() {
        val normalizer = InitialLineBreakNormalizer()
        assertEquals("first", normalizer.observe("first"))
        assertEquals(
            "${TextApiClient.WIPE_SIGNAL}second",
            normalizer.observe("${TextApiClient.WIPE_SIGNAL}\r\nsecond"),
        )
    }
}

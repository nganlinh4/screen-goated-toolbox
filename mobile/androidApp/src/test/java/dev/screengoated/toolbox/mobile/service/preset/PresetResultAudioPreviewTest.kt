package dev.screengoated.toolbox.mobile.service.preset

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetResultAudioPreviewTest {
    @Test
    fun capabilityIsPerResultAndRevokedOnReplacement() {
        val first = ResultAudioPreview()
        val second = ResultAudioPreview()
        val oldUrl = requireNotNull(first.replace(byteArrayOf(1)))
        assertTrue(first.accepts(oldUrl, "GET", false))
        assertFalse(first.accepts(oldUrl, "POST", false))
        assertFalse(first.accepts(oldUrl, "GET", true))
        assertFalse(second.accepts(oldUrl, "GET", false))
        val newUrl = requireNotNull(first.replace(byteArrayOf(2)))
        assertFalse(first.accepts(oldUrl, "GET", false))
        assertTrue(first.accepts(newUrl, "GET", false))
        first.replace(null)
        assertFalse(first.accepts(newUrl, "GET", false))
    }

    @Test
    fun servesBoundedRangesAndRejectsInvalidRequests() {
        assertEquals(AudioPreviewRange(0, 99), audioPreviewRange(null, 100))
        assertEquals(AudioPreviewRange(10, 19), audioPreviewRange("bytes=10-19", 100))
        assertEquals(AudioPreviewRange(90, 99), audioPreviewRange("bytes=-10", 100))
        assertEquals(AudioPreviewRange(90, 99), audioPreviewRange("bytes=90-1000", 100))
        assertEquals(AudioPreviewRange(90, 99), audioPreviewRange("bytes=90-", 100))
        listOf("bytes=100-", "bytes=10-9", "bytes=-0", "bytes=1-2,4-5", "bytes=-", "bytes=999999999999999999999-")
            .forEach { assertNull(audioPreviewRange(it, 100)) }
        assertNull(audioPreviewRange(null, 0))
    }
}

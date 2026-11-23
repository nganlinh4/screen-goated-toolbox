package dev.screengoated.toolbox.mobile.capture

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class CaptureSetupReleaseTest {
    @Test
    fun setupFailure_releasesAndRethrows() {
        val released = mutableListOf<String>()
        val failure = IllegalStateException("Playback AudioRecord failed to initialize.")
        try {
            setUpCaptureOrRelease<Unit>(release = { released += "record"; released += "projection" }) {
                throw failure
            }
            fail("setup failure must propagate")
        } catch (error: IllegalStateException) {
            assertSame(failure, error)
        }
        assertEquals(listOf("record", "projection"), released)
    }

    @Test
    fun releaseFailure_isSuppressedIntoTheSetupFailure() {
        val failure = IllegalStateException("start failed")
        try {
            setUpCaptureOrRelease<Unit>(release = { throw IllegalArgumentException("release failed") }) {
                throw failure
            }
            fail("setup failure must propagate")
        } catch (error: IllegalStateException) {
            assertSame(failure, error)
            assertEquals("release failed", error.suppressedExceptions.single().message)
        }
    }

    @Test
    fun successfulSetup_keepsResources() {
        var released = false
        assertEquals(7, setUpCaptureOrRelease(release = { released = true }) { 7 })
        assertFalse(released)
    }

    @Test
    fun playbackInitFailure_releasesProjectionAndRecord() {
        val source = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/capture/AudioCaptureController.kt",
        ).readText()
        val playback = source.substringAfter("private fun <T> playbackFlow(").substringBefore("val reader = launch(")
        val release = playback.substringAfter("setUpCaptureOrRelease(release = {").substringBefore("}) {")
        assertTrue(release.contains("setupRecord?.release()"))
        assertTrue(release.contains("mediaProjection.stop()"))
        assertTrue(release.contains("projectionConsentStore.clear()"))
        val setup = playback.substringAfter("}) {")
        for (step in listOf("check(minBufferSize > 0)", "AudioRecord.Builder()", "STATE_INITIALIZED", "startRecording()")) {
            assertTrue("$step must run inside the released setup", setup.contains(step))
        }
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}

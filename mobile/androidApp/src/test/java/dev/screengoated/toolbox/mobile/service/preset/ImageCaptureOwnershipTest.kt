package dev.screengoated.toolbox.mobile.service.preset

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.*
import org.junit.Test

class ImageCaptureOwnershipTest {
    @Test fun sharedLifecycleCasesFenceOldOwners() {
        val path = listOf(
            File("../../parity-fixtures/preset-system/continuous-image-lifecycle.json"),
            File("../parity-fixtures/preset-system/continuous-image-lifecycle.json"),
            File("parity-fixtures/preset-system/continuous-image-lifecycle.json"),
        ).first(File::isFile)
        val cases = Json.parseToJsonElement(path.readText()).jsonObject.getValue("cases").jsonArray
        for (item in cases.map { it.jsonObject }) {
            val lifecycle = ImageCaptureOwnership()
            when (item.getValue("event").jsonPrimitive.content) {
                "stop_during_startup" -> {
                    val old = lifecycle.start()
                    lifecycle.stop()
                    assertEquals(item.getValue("old_input_allowed").jsonPrimitive.boolean, lifecycle.isCurrent(old))
                    assertEquals(item.getValue("active_after_retirement").jsonPrimitive.boolean, lifecycle.active)
                }
                "startup_failure" -> {
                    val old = lifecycle.start()
                    assertNotNull(lifecycle.retire(old))
                    assertEquals(item.getValue("old_input_allowed").jsonPrimitive.boolean, lifecycle.isCurrent(old))
                    assertEquals(item.getValue("active_after_retirement").jsonPrimitive.boolean, lifecycle.active)
                }
                "replacement_during_retirement" -> {
                    val old = lifecycle.start()
                    assertTrue(lifecycle.beginScreenshot(old))
                    val replacement = lifecycle.start()
                    assertEquals(item.getValue("overlap_allowed").jsonPrimitive.boolean, lifecycle.canStart(replacement))
                    assertTrue(lifecycle.finishScreenshot(old))
                    assertEquals(item.getValue("replacement_after_retirement").jsonPrimitive.boolean, lifecycle.canStart(replacement))
                }
                "stale_retirement" -> {
                    val old = lifecycle.start()
                    val replacement = lifecycle.start()
                    assertNull(lifecycle.retire(old))
                    assertEquals(item.getValue("replacement_remains_active").jsonPrimitive.boolean, lifecycle.isCurrent(replacement))
                }
                "stop_active_drag" -> {
                    // Touch selection has no desktop right-button hook; Stop retires its owner.
                    val owner = lifecycle.start()
                    lifecycle.stop()
                    assertFalse(lifecycle.isCurrent(owner))
                    assertFalse(lifecycle.active)
                }
                else -> error("unhandled lifecycle fixture")
            }
        }
    }

    @Test fun latestReplacementWaitsForStaleScreenshotAndStopDiscardsIt() {
        val lifecycle = ImageCaptureOwnership()
        val old = lifecycle.start()
        assertTrue(lifecycle.beginScreenshot(old))
        val replaced = lifecycle.start()
        val latest = lifecycle.start()
        assertFalse(lifecycle.canStart(replaced))
        assertFalse(lifecycle.canStart(latest))
        assertFalse(lifecycle.finishScreenshot(replaced))
        assertTrue(lifecycle.finishScreenshot(old))
        assertTrue(lifecycle.canStart(latest))
        assertNull(lifecycle.retire(old))
        assertTrue(lifecycle.isCurrent(latest))
        lifecycle.stop()
        assertFalse(lifecycle.canStart(latest))
    }

    @Test fun retiredRevisionCannotRestoreReplacementSurface() {
        val lifecycle = ImageCaptureOwnership()
        val first = lifecycle.start()
        val retiring = lifecycle.retire(first)!!
        val replacement = lifecycle.start()
        assertFalse(lifecycle.isRevision(retiring))
        assertTrue(lifecycle.isCurrent(replacement))
    }

    @Test fun failedOverlayDetachDoesNotRestoreOrAdmitReplacement() {
        val lifecycle = ImageCaptureOwnership()
        lifecycle.start()
        val replacement = lifecycle.start()
        val restored = mutableListOf<String>()
        var retainedBitmap = true
        val detached = retireCaptureUi(
            detach = { false },
            stillCurrent = { lifecycle.isCurrent(replacement) },
            restoreBubble = { restored += "bubble" },
            restoreOverlay = { restored += "overlay" },
        )
        if (detached) retainedBitmap = false
        assertFalse(detached)
        assertTrue(retainedBitmap)
        assertTrue(restored.isEmpty())
        assertNotNull(lifecycle.retire(replacement))
        assertFalse(lifecycle.active)
    }

    @Test fun replacementCannotStartBeforeEarlierShowReturns() {
        val lifecycle = ImageCaptureOwnership()
        val old = lifecycle.start()
        assertTrue(lifecycle.beginShow(old))
        val replacement = lifecycle.start()
        assertTrue(lifecycle.showInFlight)
        assertFalse(lifecycle.canStart(replacement))
        assertTrue(lifecycle.finishShow(old))
        assertFalse(lifecycle.showInFlight)
        assertTrue(lifecycle.canStart(replacement))
        assertNull(lifecycle.retire(old))
        assertTrue(lifecycle.isCurrent(replacement))
    }

    @Test fun retainedCloseRetriesOnlyAfterRealDetachment() {
        var attached = true
        var recycled = false
        var allowDetach = false
        var replacements = 0
        var restores = 0
        var staleSelections = 0
        var failures = 0
        val detach = {
            if (!allowDetach) false else {
                attached = false
                recycled = true
                true
            }
        }
        val afterDetached: () -> Unit = {
            assertFalse(attached)
            assertTrue(recycled)
            replacements++
            restores++
        }
        val failure: () -> Unit = { failures++ }
        assertFalse(retryRetainedCapture(7, 8, false, detach, afterDetached, failure))
        assertFalse(retryRetainedCapture(7, 7, true, detach, afterDetached, failure))
        assertFalse(retryRetainedCapture(7, 7, false, detach, afterDetached, failure))
        assertTrue(attached)
        assertFalse(recycled)
        assertEquals(0, replacements)
        assertEquals(0, restores)
        assertEquals(0, staleSelections)
        assertEquals(1, failures)
        allowDetach = true
        assertTrue(retryRetainedCapture(7, 7, false, detach, afterDetached, failure))
        assertEquals(1, replacements)
        assertEquals(1, restores)
        assertEquals(0, staleSelections)
    }
}

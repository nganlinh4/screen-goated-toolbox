package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import java.io.File
import java.lang.ref.WeakReference
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetResultMediaActionsTest {
    private fun fixture(): kotlinx.serialization.json.JsonObject {
        var root = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(root, ".claude/parity").exists()) root = requireNotNull(root.parentFile)
        return Json.parseToJsonElement(File(root, "parity-fixtures/preset-system/result-media-actions.json").readText()).jsonObject
    }

    @Test
    fun mediaActionsFollowSharedContractAndNeverCopyMediaHtml() {
        val original = String(charArrayOf('m', 'e', 'd', 'i', 'a'))
        val bytes = byteArrayOf(1, 2, 3)
        for (item in fixture().getValue("cases").jsonArray) {
            val case = item.jsonObject
            val kind = case.getValue("kind").jsonPrimitive.content
            val isOriginal = case.getValue("originalContent").jsonPrimitive.boolean
            val state = PresetResultWindowState(
                id = PresetResultWindowId("session", 1), blockIdx = 1, title = "result",
                markdownText = if (isOriginal) original else "replaced",
                inputImageBytes = if (kind == "image") bytes else null,
                inputImageHtml = if (kind == "image") WeakReference(original) else null,
                inputAudioBytes = if (kind == "audio") bytes else null,
                inputAudioHtml = if (kind == "audio") WeakReference(original) else null,
            )
            assertEquals(case.getValue("disabled").jsonArray.map { it.jsonPrimitive.content }.toSet(), resultDisabledActions(state))
            var action = "disabled"
            val copied = copyResultPayload(state, { action = "text"; true }, { image ->
                assertSame(bytes, image)
                action = "image_uri"
                true
            })
            assertEquals(case.getValue("copy").jsonPrimitive.content, action)
            assertEquals(action != "disabled", copied)
        }
    }

    @Test
    fun largeTextAndClipboardFailureDoNotReportSuccess() {
        val limit = fixture().getValue("textCopy").jsonObject.getValue("maximumUtf16Units").jsonPrimitive.int
        assertEquals(MAX_RESULT_CLIPBOARD_UTF16_UNITS, limit)
        val state = PresetResultWindowState(PresetResultWindowId("s", 0), 0, "result", markdownText = "a".repeat(limit + 1))
        var writes = 0
        assertFalse(copyResultPayload(state, { writes++; true }, { writes++; true }))
        assertEquals(0, writes)
        assertTrue(copyResultPayload(state.copy(markdownText = "a".repeat(limit)), { true }, { false }))
        assertFalse(copyResultPayload(state.copy(markdownText = "text"), { throw IllegalStateException("clipboard") }, { false }))
    }
}

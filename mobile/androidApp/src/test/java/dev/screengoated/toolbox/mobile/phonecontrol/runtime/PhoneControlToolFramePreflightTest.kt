package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PhoneControlToolFramePreflightTest {
    @Test
    fun `normal multilingual tool frame is accepted`() {
        assertNull(
            PhoneControlToolFramePreflight.rejection(
                listOf(call("작업-1", "quan sát", JsonObject(emptyMap()))),
            ),
        )
    }

    @Test
    fun `call count is bounded before any dispatch or rejection response`() {
        val calls = List(PhoneControlToolFramePreflight.MAXIMUM_CALLS + 1) { index ->
            call("call-$index", "observe", JsonObject(emptyMap()))
        }
        assertEquals(
            PhoneControlToolFrameRejection.TOO_MANY_CALLS,
            PhoneControlToolFramePreflight.rejection(calls),
        )
    }

    @Test
    fun `identity and argument UTF8 bytes are bounded structurally`() {
        assertEquals(
            PhoneControlToolFrameRejection.ID_TOO_LARGE,
            PhoneControlToolFramePreflight.rejection(
                listOf(call("界".repeat(342), "observe", JsonObject(emptyMap()))),
            ),
        )
        assertEquals(
            PhoneControlToolFrameRejection.NAME_TOO_LARGE,
            PhoneControlToolFramePreflight.rejection(
                listOf(call("id", "界".repeat(342), JsonObject(emptyMap()))),
            ),
        )
        assertEquals(
            PhoneControlToolFrameRejection.ARGUMENTS_TOO_LARGE,
            PhoneControlToolFramePreflight.rejection(
                listOf(
                    call(
                        "id",
                        "observe",
                        JsonPrimitive(
                            "x".repeat(PhoneControlToolFramePreflight.MAXIMUM_ARGUMENTS_UTF8_BYTES),
                        ),
                    ),
                ),
            ),
        )
    }

    private fun call(id: String, name: String, args: kotlinx.serialization.json.JsonElement) =
        GeminiLiveFunctionCall(id, name, args)
}

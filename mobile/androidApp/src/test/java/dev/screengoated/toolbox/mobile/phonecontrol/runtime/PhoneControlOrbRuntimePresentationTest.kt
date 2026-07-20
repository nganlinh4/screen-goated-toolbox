package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PhoneControlOrbRuntimePresentationTest {
    @Test
    fun `scroll tools use every generated directional icon`() {
        val expected = mapOf(
            "up" to "keyboard_double_arrow_up",
            "down" to "keyboard_double_arrow_down",
            "left" to "keyboard_double_arrow_left",
            "right" to "keyboard_double_arrow_right",
        )

        expected.forEach { (direction, icon) ->
            val presentation = call(
                name = "scroll",
                args = buildJsonObject { put("direction", direction) },
            ).orbPresentation()

            assertEquals("scroll", presentation.stateLabel)
            assertEquals(icon, presentation.iconOverride)
        }
    }

    @Test
    fun `non-scroll tool keeps its generated state without an icon override`() {
        val presentation = call(name = "click_target").orbPresentation()

        assertEquals("click", presentation.stateLabel)
        assertNull(presentation.iconOverride)
    }

    private fun call(
        name: String,
        args: kotlinx.serialization.json.JsonElement = kotlinx.serialization.json.JsonNull,
    ) = GeminiLiveFunctionCall(id = "call-1", name = name, args = args)
}

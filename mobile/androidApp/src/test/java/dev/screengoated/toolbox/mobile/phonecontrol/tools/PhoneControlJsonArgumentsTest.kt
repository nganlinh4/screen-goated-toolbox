package dev.screengoated.toolbox.mobile.phonecontrol.tools

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PhoneControlJsonArgumentsTest {
    private val wrongShapes = Json.parseToJsonElement(
        """{"id":[1,2],"x":{"v":1},"flag":[true],"name":{"s":"a"}}""",
    ).jsonObject

    @Test
    fun intOnArray_returnsNull() {
        assertNull(wrongShapes.int("id"))
    }

    @Test
    fun numberOnObject_returnsNull() {
        assertNull(wrongShapes.number("x"))
    }

    @Test
    fun booleanOnArray_returnsNull() {
        assertNull(wrongShapes.boolean("flag"))
    }

    @Test
    fun stringOnObject_returnsNull() {
        assertNull(wrongShapes.string("name"))
    }

    @Test
    fun primitivesStillDecode() {
        val args = Json.parseToJsonElement(
            """{"id":7,"x":1.5,"flag":true,"name":"a","missing":null}""",
        ).jsonObject
        assertEquals(7, args.int("id"))
        assertEquals(1.5, args.number("x")!!, 0.0)
        assertEquals(true, args.boolean("flag"))
        assertEquals("a", args.string("name"))
        assertNull(args.int("missing"))
        assertNull(args.int("absent"))
    }
}

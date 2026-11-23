package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.session.validatedPhoneControlDeclarations
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.*
import org.junit.Test

class ResourceBoundaryContractTest {
    private fun declarations() = JsonArray(PhoneControlToolRegistry.specs.map { spec -> buildJsonObject {
        put("name", spec.name); put("parameters", JsonObject(emptyMap()))
    } })

    @Test fun validatedSessionDeclarationsAreExactUniqueAndKeepTheirSchemas() {
        val source = declarations()
        val validated = validatedPhoneControlDeclarations(source)
        assertEquals(PhoneControlToolRegistry.byName.keys, validated.keys)
        assertSame(source.first(), validated.getValue(PhoneControlToolRegistry.specs.first().name))
        for (invalid in listOf(JsonArray(source.drop(1)), JsonArray(source + source.first()), JsonArray(source.drop(1) + buildJsonObject { put("name", "future_tool"); put("parameters", JsonObject(emptyMap())) }))) {
            assertTrue(runCatching { validatedPhoneControlDeclarations(invalid) }.isFailure)
        }
    }

    @Test fun commandExecutionAndResourceAssessmentShareExactAndShellParsing() {
        val exact = parseCommandRequest(buildJsonObject {
            put("program", "resource-tool"); put("cwd", "/work")
            put("args", buildJsonArray { add(JsonPrimitive("a literal argument")) })
        })
        assertEquals(listOf("a literal argument"), exact.argv)
        assertEquals("/work", exact.proposal().string("cwd"))
        val shell = parseCommandRequest(buildJsonObject { put("command", "inspect '$RESOURCE'") })
        assertEquals("/system/bin/sh", shell.program)
        assertEquals(listOf("-c", "inspect '$RESOURCE'"), shell.argv)
        assertEquals("/data/local/tmp", shell.cwd)
        assertTrue(runCatching { parseCommandRequest(buildJsonObject { put("program", "a"); put("command", "b") }) }.isFailure)
    }

    companion object { private const val RESOURCE = "literal-protected-path" }
}

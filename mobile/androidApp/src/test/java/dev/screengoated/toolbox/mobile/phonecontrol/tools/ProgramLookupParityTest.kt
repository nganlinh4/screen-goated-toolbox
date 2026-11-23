package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.commandScript
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.shellQuote
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ProgramLookupParityTest {
    @Test
    fun exactProgramsPreserveNativeLookupAndExplicitPathsAcrossWorkingDirectories() {
        val path = "parity-fixtures/phone-control/program-lookup.json"
        val fixture = sequenceOf(File(path), File("../$path"), File("../../$path"))
            .first(File::isFile).readText()
        val cases = Json.parseToJsonElement(fixture).jsonObject.getValue("cases").jsonArray
        for (case in cases.map { it.jsonObject }) {
            val program = case.getValue("androidProgram").jsonPrimitive.content
            for (cwd in listOf("/data/local/tmp/first", "/data/local/tmp/second")) {
                val request = parseCommandRequest(buildJsonObject {
                    put("program", program)
                    put("cwd", cwd)
                })
                assertEquals(program, request.program)
                val script = commandScript(request.program, request.argv, request.cwd, "__STATUS__")
                assertTrue(script.startsWith("cd ${shellQuote(cwd)} || exit 125; ${shellQuote(program)};"))
            }
        }
    }
}

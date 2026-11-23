package dev.screengoated.toolbox.mobile.phonecontrol

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlLogEventCoverageTest {
    @Test
    fun everyEmittedEventIsAllowlisted() {
        val sourceRoot = repoFile("mobile/androidApp/src/main/java")
        val emitted = sourceRoot.walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            .flatMap { file -> emittedEventTokens(file.readText()).map { token -> file.name to token } }
            .toList()
        assertTrue("the scan must find the phone-control diagnostics", emitted.size > 100)

        val rewritten = emitted.filter { (_, token) ->
            PhoneControlLog.parseDiagnosticEvent(token).name != token
        }
        assertEquals("events rewritten to diagnostic_event", emptyList<Pair<String, String>>(), rewritten)
    }

    /** The literal first token of each message; templates that build the name at runtime are skipped. */
    private fun emittedEventTokens(source: String): List<String> {
        val call = if (source.contains("PhoneControlLog as Log")) ALIASED_CALL else DIRECT_CALL
        return call.findAll(source)
            .filterNot { match -> match.groupValues[2] == "$" }
            .map { match -> match.groupValues[1] }
            .filter(String::isNotEmpty)
            .toList()
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }

    private companion object {
        val DIRECT_CALL = Regex("""PhoneControlLog\.[diwe]\(\s*[^,]+,\s*"([^"$ ]*)(.?)""")
        val ALIASED_CALL = Regex("""(?:PhoneControlLog|\bLog)\.[diwe]\(\s*[^,]+,\s*"([^"$ ]*)(.?)""")
    }
}

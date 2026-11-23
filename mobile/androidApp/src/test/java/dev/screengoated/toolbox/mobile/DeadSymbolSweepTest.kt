package dev.screengoated.toolbox.mobile

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class DeadSymbolSweepTest {
    @Test
    fun sweptDeclarationsStayDeleted() {
        val main = "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile"
        assertFalse(repoFile("$main/preset/ui/ModelCatalog.kt").exists())
        assertFalse(repoFile("$main/phonecontrol/provider/PhoneControlStorageGrants.kt").exists())

        val declarations = listOf(
            "fun isGeminiTranslateModelId(",
            "fun rankedModels(",
            "fun voicesForLanguage(",
            "fun getPresetsByType(",
        )
        val offenders = SOURCE_ROOTS.asSequence()
            .map { repoFile(it) }
            .filter(File::isDirectory)
            .flatMap { root -> root.walkTopDown().filter { it.isFile && it.extension == "kt" } }
            .flatMap { file ->
                val source = file.readText()
                declarations.filter(source::contains).map { "${file.name}: $it" }
            }
            .toList()
        assertEquals(emptyList<String>(), offenders)
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }

    private companion object {
        val SOURCE_ROOTS = listOf("main", "full", "play", "debug")
            .map { "mobile/androidApp/src/$it/java" }
    }
}

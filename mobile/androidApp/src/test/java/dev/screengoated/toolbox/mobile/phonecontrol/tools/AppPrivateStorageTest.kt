package dev.screengoated.toolbox.mobile.phonecontrol.tools

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AppPrivateStorageTest {
    @Test
    fun privateRootAndItsDescendantsAreRecognizedAfterNormalization() {
        val base = Files.createTempDirectory("sgt-private").toFile()
        try {
            val files = File(base, "files").apply { mkdirs() }
            val shared = File(base, "shared").apply { mkdirs() }
            val memory = File(files, "phone_control/memory.json")
            val traversal = File(shared, "../files/phone_control/memory.json")

            assertTrue(files.isWithinAnyRoot(listOf(files)))
            assertTrue(memory.isWithinAnyRoot(listOf(files)))
            assertTrue(traversal.isWithinAnyRoot(listOf(files)))
            assertFalse(File(shared, "photo.jpg").isWithinAnyRoot(listOf(files)))
            assertFalse(File(base, "files-other/a.txt").isWithinAnyRoot(listOf(files)))
        } finally {
            base.deleteRecursively()
        }
    }

    @Test
    fun ownProviderAuthoritiesAreRefusedButOthersAreNot() {
        val pkg = "dev.screengoated.toolbox.mobile"
        assertTrue(isOwnContentAuthority("$pkg.fileprovider", pkg))
        assertTrue(isOwnContentAuthority("DEV.SCREENGOATED.TOOLBOX.MOBILE.FileProvider", pkg))
        assertTrue(isOwnContentAuthority(pkg, pkg))
        assertFalse(isOwnContentAuthority("com.android.externalstorage.documents", pkg))
        assertFalse(isOwnContentAuthority("${pkg}x.fileprovider", pkg))
        assertFalse(isOwnContentAuthority(null, pkg))
    }

    @Test
    fun resourceLaunchChecksPrivateStorageBeforeGranting() {
        val source = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/tools/ResourceLaunchToolHandler.kt",
        ).readText()
        val openPath = source.substringAfter("private suspend fun openPath(")
        assertTrue(
            openPath.indexOf("appPrivateStorageRoots(context)") in 0 until openPath.indexOf("ordinaryResourceUri(canonical)"),
        )
        val openContent = source.substringAfter("private fun openContent(")
        assertTrue(
            openContent.indexOf("isOwnContentAuthority") in 0 until openContent.indexOf("app.openResource"),
        )
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }
}

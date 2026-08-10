package dev.screengoated.toolbox.mobile.service.moonshine

import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import java.io.File
import java.nio.file.Files
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

class MoonshineBundleInstallerTest {
    private val contract = MoonshineModelFile(
        "model.ort",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )

    @Test
    fun verifiedBundleInstallsOnlyItsExactModelEntries() = runTest {
        withDirectory { directory ->
            val archive = File(directory, "bundle.zip")
            writeArchive(archive, includeUnexpected = false)
            val bundle = bundleFor(archive)
            val modelDirectory = File(directory, "model")
            val installer = MoonshineBundleInstaller(OkHttpClient(), File(directory, "cache"))

            installer.installVerifiedArchive(bundle, listOf(contract), archive, modelDirectory)

            assertEquals("abc", File(modelDirectory, contract.name).readText())
            assertFalse(File(modelDirectory, "LICENSE.txt").exists())
            assertFalse(File(modelDirectory, "NOTICE.txt").exists())
        }
    }

    @Test
    fun unexpectedEntryCannotReplaceExistingModel() = runTest {
        withDirectory { directory ->
            val archive = File(directory, "bundle.zip")
            writeArchive(archive, includeUnexpected = true)
            val modelDirectory = File(directory, "model").apply { mkdirs() }
            val target = File(modelDirectory, contract.name).apply { writeText("old") }
            val installer = MoonshineBundleInstaller(OkHttpClient(), File(directory, "cache"))

            val failure = runCatching {
                installer.installVerifiedArchive(
                    bundleFor(archive),
                    listOf(contract),
                    archive,
                    modelDirectory,
                )
            }.exceptionOrNull()

            check(failure != null)
            assertEquals("old", target.readText())
        }
    }

    private fun writeArchive(target: File, includeUnexpected: Boolean) {
        ZipOutputStream(target.outputStream()).use { archive ->
            writeEntry(archive, contract.name, "abc".toByteArray())
            writeEntry(archive, "LICENSE.txt", ByteArray(13_555) { 1 })
            writeEntry(archive, "NOTICE.txt", ByteArray(804) { 2 })
            if (includeUnexpected) writeEntry(archive, "extra.txt", byteArrayOf(1))
        }
    }

    private fun writeEntry(archive: ZipOutputStream, name: String, bytes: ByteArray) {
        archive.putNextEntry(ZipEntry(name))
        archive.write(bytes)
        archive.closeEntry()
    }

    private fun bundleFor(archive: File) = MoonshineModelBundle(
        "test.zip",
        archive.length(),
        ManagedModelIntegrity.sha256(archive),
        "https://example.invalid/test.zip",
    )

    private suspend fun withDirectory(test: suspend (File) -> Unit) {
        val directory = Files.createTempDirectory("moonshine-bundle-").toFile()
        try {
            test(directory)
        } finally {
            directory.deleteRecursively()
        }
    }
}

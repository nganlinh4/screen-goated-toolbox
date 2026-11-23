package dev.screengoated.toolbox.mobile.storage

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class LegacyOfflineModelsTest {
    @get:Rule
    val temporary = TemporaryFolder()

    private lateinit var filesDir: File
    private lateinit var cacheDir: File
    private val deletedPreferences = mutableListOf<String>()

    private fun legacy(): LegacyOfflineModels {
        filesDir = File(temporary.root, "files").apply { mkdirs() }
        cacheDir = File(temporary.root, "cache").apply { mkdirs() }
        return LegacyOfflineModels(filesDir, cacheDir) { deletedPreferences += it }
    }

    private fun write(file: File, bytes: Int): File {
        file.parentFile!!.mkdirs()
        file.writeBytes(ByteArray(bytes))
        return file
    }

    @Test
    fun `no legacy folder hides the cleanup row`() {
        val legacy = legacy()
        write(File(filesDir, "models/other/keep.bin"), 10)
        assertNull(legacy.presentBytes())
    }

    @Test
    fun `size covers only the known legacy folders and nothing is removed before the click`() {
        val legacy = legacy()
        write(File(filesDir, "models/moonshine/tiny-streaming-en/encoder.ort"), 100)
        write(File(filesDir, "models/moonshine/streaming-zipformer-en-kroko/joiner.onnx"), 20)
        write(File(filesDir, "native-libs/libsherpa-onnx-jni.so"), 3)
        write(File(cacheDir, "moonshine-model-bundles/bundle.zip.part"), 4)
        val unrelated = write(File(filesDir, "models/other/keep.bin"), 1_000)

        assertEquals(127L, legacy.presentBytes())
        assertTrue(File(filesDir, "models/moonshine/tiny-streaming-en/encoder.ort").isFile)
        assertTrue(unrelated.isFile)
    }

    @Test
    fun `remove deletes exactly the legacy folders and their bookkeeping`() {
        val legacy = legacy()
        write(File(filesDir, "models/moonshine/tiny-streaming-en/encoder.ort"), 100)
        write(File(filesDir, "native-libs/libonnxruntime.so"), 3)
        write(File(cacheDir, "moonshine-model-bundles/bundle.zip.part"), 4)
        val unrelatedModel = write(File(filesDir, "models/other/keep.bin"), 1)
        val unrelatedFile = write(File(filesDir, "creation/history.json"), 1)
        val unrelatedCache = write(File(cacheDir, "creation/keep.tmp"), 1)

        assertTrue(legacy.remove())

        assertFalse(File(filesDir, "models/moonshine").exists())
        assertFalse(File(filesDir, "native-libs").exists())
        assertFalse(File(cacheDir, "moonshine-model-bundles").exists())
        assertTrue(unrelatedModel.isFile)
        assertTrue(unrelatedFile.isFile)
        assertTrue(unrelatedCache.isFile)
        assertNull(legacy.presentBytes())
        assertEquals(
            listOf("downloaded_model_lifecycle", "downloaded_native_runtime_lifecycle"),
            deletedPreferences,
        )
    }

    @Test
    fun `an emptied models folder is removed`() {
        val legacy = legacy()
        write(File(filesDir, "models/moonshine/tiny-streaming-en/encoder.ort"), 1)
        assertTrue(legacy.remove())
        assertFalse(File(filesDir, "models").exists())
    }

    @Test
    fun `links inside or as a legacy folder are removed without touching their targets`() {
        val legacy = legacy()
        val outside = write(File(temporary.root, "outside/user-data.bin"), 7)
        val linkInside = File(filesDir, "models/moonshine/escape")
        linkInside.parentFile!!.mkdirs()
        val linkAsFolder = File(filesDir, "native-libs")
        val linked = runCatching {
            Files.createSymbolicLink(linkInside.toPath(), outside.parentFile!!.toPath())
            Files.createSymbolicLink(linkAsFolder.toPath(), outside.parentFile!!.toPath())
        }.isSuccess
        assumeTrue("Symbolic links are unavailable on this host", linked)

        assertEquals("links are never followed when sizing", 0L, legacy.presentBytes())
        assertTrue(legacy.remove())

        assertTrue(outside.isFile)
        assertEquals(7L, outside.length())
        assertFalse(Files.exists(linkInside.toPath(), java.nio.file.LinkOption.NOFOLLOW_LINKS))
        assertFalse(Files.exists(linkAsFolder.toPath(), java.nio.file.LinkOption.NOFOLLOW_LINKS))
    }
}

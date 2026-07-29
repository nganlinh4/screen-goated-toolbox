package dev.screengoated.toolbox.mobile.creation.runtime

import dev.screengoated.toolbox.mobile.creation.deleteCreationTreeNoFollow
import java.io.File
import java.nio.file.Files
import kotlin.io.path.createTempDirectory
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class CreationRuntimeDeliveryTest {
    @Test
    fun `private delivery metadata is parsed without checked in constants`() {
        val delivery = parseCreationRuntimeDelivery(manifest())

        assertEquals("1.2.3", delivery.version)
        assertEquals(2, delivery.entries.size)
        assertEquals("runtime/factory.dex", delivery.entry(ROLE_FACTORY_DEX).archivePath)
        assertEquals("lib/runtime.so", delivery.entry(ROLE_NATIVE_LIBRARY).installPath)
    }

    @Test
    fun `install paths cannot escape the runtime directory`() {
        val manifest = manifest()
        manifest.getJSONObject("android")
            .getJSONArray("entries")
            .getJSONObject(0)
            .put("installPath", "../outside.dex")

        assertThrows(IllegalArgumentException::class.java) {
            parseCreationRuntimeDelivery(manifest)
        }
    }

    @Test
    fun `delivery manifest caps executable entries`() {
        val manifest = manifest()
        val entries = manifest.getJSONObject("android").getJSONArray("entries")
        repeat(63) { index ->
            entries.put(
                entry(
                    "extra-$index",
                    "extra/$index.bin",
                    "extra/$index.bin",
                    "a".repeat(64),
                ),
            )
        }

        assertThrows(IllegalArgumentException::class.java) {
            parseCreationRuntimeDelivery(manifest)
        }
    }

    @Test
    fun `stale runtime cleanup is confined and never follows links`() {
        val parent = createTempDirectory("creation-runtime-parent").toFile()
        val runtime = File(parent, "runtime").apply { mkdirs() }
        val stale = File(runtime, "stale").apply { mkdirs() }
        File(stale, "factory.dex").writeText("old")
        val outside = createTempDirectory("creation-runtime-outside").toFile()
        val outsideFile = File(outside, "keep").apply { writeText("keep") }
        val link = File(stale, "outside-link")
        runCatching { Files.createSymbolicLink(link.toPath(), outside.toPath()) }

        assertEquals(true, deleteCreationTreeNoFollow(runtime, stale))
        assertEquals(false, stale.exists())
        assertEquals(true, outsideFile.isFile)

        runtime.delete()
        parent.delete()
        outsideFile.delete()
        outside.delete()
    }

    private fun manifest(): JSONObject {
        val hash = "a".repeat(64)
        val entries = JSONArray()
            .put(entry("factory_dex", "runtime/factory.dex", "factory.dex", hash))
            .put(entry("native_library", "lib/runtime.so", "lib/runtime.so", hash))
        return JSONObject()
            .put("version", "1.2.3")
            .put(
                "android",
                JSONObject()
                    .put("factoryClass", "example.RuntimeFactory")
                    .put(
                        "full",
                        JSONObject()
                            .put("asset", "runtime.zip")
                            .put("downloadUrl", "https:" + "//runtime.invalid/bundle")
                            .put("sizeBytes", 128)
                            .put("sha256", hash),
                    )
                    .put("entries", entries),
            )
    }

    private fun entry(
        role: String,
        archivePath: String,
        installPath: String,
        hash: String,
    ): JSONObject = JSONObject()
        .put("role", role)
        .put("archivePath", archivePath)
        .put("installPath", installPath)
        .put("sizeBytes", 64)
        .put("sha256", hash)
}

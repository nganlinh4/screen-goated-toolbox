package dev.screengoated.toolbox.mobile.creation

import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.file.Files
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationWireframeGlbTest {
    @Test
    fun `triangle primitives become deduplicated line geometry`() {
        val directory = Files.createTempDirectory("creation-wireframe").toFile()
        val source = directory.resolve("source.glb").apply { writeBytes(triangleGlb()) }
        val target = directory.resolve("wireframe.glb")

        CreationWireframeGlb.create(source, target)

        val bytes = target.readBytes()
        val header = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        assertEquals(0x46546c67, header.int)
        assertEquals(2, header.int)
        assertEquals(bytes.size, header.int)
        val jsonLength = header.int
        assertEquals(0x4e4f534a, header.int)
        val document = JSONObject(
            bytes.copyOfRange(20, 20 + jsonLength)
                .toString(Charsets.UTF_8)
                .trimEnd(' ', '\u0000'),
        )
        val primitive = document.getJSONArray("meshes")
            .getJSONObject(0)
            .getJSONArray("primitives")
            .getJSONObject(0)
        assertEquals(1, primitive.getInt("mode"))
        val lineAccessor = document.getJSONArray("accessors")
            .getJSONObject(primitive.getInt("indices"))
        assertEquals(6, lineAccessor.getInt("count"))
        assertEquals(5_123, lineAccessor.getInt("componentType"))
        assertTrue(target.length() > source.length())
        directory.deleteRecursively()
    }

    private fun triangleGlb(): ByteArray {
        val document = JSONObject(
            """
            {
              "asset":{"version":"2.0"},
              "buffers":[{"byteLength":42}],
              "bufferViews":[
                {"buffer":0,"byteOffset":0,"byteLength":36,"target":34962},
                {"buffer":0,"byteOffset":36,"byteLength":6,"target":34963}
              ],
              "accessors":[
                {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
                {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}
              ],
              "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
              "nodes":[{"mesh":0}],
              "scenes":[{"nodes":[0]}],
              "scene":0
            }
            """.trimIndent(),
        )
        val binary = ByteBuffer.allocate(44).order(ByteOrder.LITTLE_ENDIAN).apply {
            putFloat(0f).putFloat(0f).putFloat(0f)
            putFloat(1f).putFloat(0f).putFloat(0f)
            putFloat(0f).putFloat(1f).putFloat(0f)
            putShort(0.toShort()).putShort(1.toShort()).putShort(2.toShort())
        }.array()
        val json = document.toString().toByteArray().let { raw ->
            raw.copyOf(raw.size + ((4 - raw.size % 4) % 4)).also { padded ->
                for (index in raw.size until padded.size) padded[index] = ' '.code.toByte()
            }
        }
        val total = 12 + 8 + json.size + 8 + binary.size
        return ByteBuffer.allocate(total).order(ByteOrder.LITTLE_ENDIAN).apply {
            putInt(0x46546c67)
            putInt(2)
            putInt(total)
            putInt(json.size)
            putInt(0x4e4f534a)
            put(json)
            putInt(binary.size)
            putInt(0x004e4942)
            put(binary)
        }.array()
    }
}

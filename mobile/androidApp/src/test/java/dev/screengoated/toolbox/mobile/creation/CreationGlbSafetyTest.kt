package dev.screengoated.toolbox.mobile.creation

import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.io.path.createTempDirectory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class CreationGlbSafetyTest {
    @Test
    fun `validator rejects network file and relative model resources`() {
        listOf("https://example.test/model.bin", "file:///data/model.bin", "model.bin")
            .forEach { uri ->
                assertRejected(
                    document(buffer = """{"byteLength":12,"uri":"$uri"}"""),
                    binary = null,
                )
            }
        assertRejected(
            document("""{"byteLength":12}""").replace(
                """"asset":{"version":"2.0"}""",
                """"asset":{"version":"2.0"},"extensions":{"future":{"uri":"https://x"}}""",
            ),
            binary = ByteArray(12),
        )
    }

    @Test
    fun `validator rejects buffer view and chunk overruns`() {
        assertRejected(
            document(
                buffer = """{"byteLength":12}""",
                bufferView = """{"buffer":0,"byteOffset":8,"byteLength":8}""",
            ),
            binary = ByteArray(12),
        )

        val valid = glb(document("""{"byteLength":12}"""), ByteArray(12))
        ByteBuffer.wrap(valid).order(ByteOrder.LITTLE_ENDIAN).putInt(12, Int.MAX_VALUE)
        assertRejectedBytes(valid)
        assertRejected(document("""{"byteLength":12}"""), ByteArray(16))
    }

    @Test
    fun `validator accepts only required zero binary alignment padding`() {
        val aligned = glb(document("""{"byteLength":37}"""), ByteArray(37))
        validateBytes(aligned)

        val nonZeroPadding = aligned.copyOf().also { it[it.lastIndex] = 1 }
        assertRejectedBytes(nonZeroPadding)
    }

    @Test
    fun `validator rejects every chunk after the optional binary chunk`() {
        val valid = glb(document("""{"byteLength":12}"""), ByteArray(12))
        val extended = valid + ints(0, 0x12345678)
        ByteBuffer.wrap(extended).order(ByteOrder.LITTLE_ENDIAN)
            .putInt(8, extended.size)

        assertRejectedBytes(extended)
    }

    @Test
    fun `validator accepts an embedded bounded buffer`() {
        val encoded = java.util.Base64.getEncoder().encodeToString(ByteArray(36))
        validateBytes(
            glb(
                document("""{"byteLength":36,"uri":"data:application/octet-stream;base64,$encoded"}"""),
                binary = null,
            ),
        )

        val exact = java.util.Base64.getEncoder().encodeToString(ByteArray(37))
        validateBytes(
            glb(
                document("""{"byteLength":37,"uri":"data:application/octet-stream;base64,$exact"}"""),
                binary = null,
            ),
        )
        val padded = java.util.Base64.getEncoder().encodeToString(ByteArray(40))
        assertRejected(
            document("""{"byteLength":37,"uri":"data:application/octet-stream;base64,$padded"}"""),
            binary = null,
        )
    }

    @Test
    fun `validator rejects out of range primitive indices and recursive nodes`() {
        val indexed = """
            {
              "asset":{"version":"2.0"},
              "buffers":[{"byteLength":40}],
              "bufferViews":[
                {"buffer":0,"byteLength":36},
                {"buffer":0,"byteOffset":36,"byteLength":3}
              ],
              "accessors":[
                {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
                {"bufferView":1,"componentType":5121,"count":3,"type":"SCALAR"}
              ],
              "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}]
            }
        """.trimIndent()
        val bytes = ByteArray(40).apply {
            this[36] = 0
            this[37] = 1
            this[38] = 3
        }
        assertRejected(indexed, bytes)

        val recursive = document("""{"byteLength":12}""").replace(
            """"meshes":""",
            """"nodes":[{"children":[1]},{"children":[0]}],"meshes":""",
        )
        assertRejected(recursive, ByteArray(12))
    }

    @Test
    fun `validator rejects sparse accessors and multi-parent scene graphs`() {
        val sparse = document("""{"byteLength":12}""").replace(
            """"count":1,"type":"VEC3"""",
            """"count":1,"type":"VEC3","sparse":{"count":1}""",
        )
        assertRejected(sparse, ByteArray(12))

        val multiParent = document("""{"byteLength":12}""").replace(
            """"meshes":""",
            """"nodes":[{"children":[2]},{"children":[2]},{}],"meshes":""",
        )
        assertRejected(multiParent, ByteArray(12))
    }

    @Test
    fun `validator accepts only triangle primitive mode`() {
        (0..6).filter { it != 4 }.forEach { mode ->
            val hostile = document("""{"byteLength":36}""").replace(
                """"mode":4""",
                """"mode":$mode""",
            )
            assertRejected(hostile, ByteArray(36))
        }
        validateBytes(glb(document("""{"byteLength":36}"""), ByteArray(36)))
    }

    @Test
    fun `validator rejects viewer-unsupported runtime work`() {
        listOf(
            """"animations":[{}]""",
            """"cameras":[{}]""",
        ).forEach { field ->
            val hostile = document("""{"byteLength":12}""").replace(
                """"asset":{"version":"2.0"}""",
                """"asset":{"version":"2.0"},$field""",
            )
            assertRejected(hostile, ByteArray(12))
        }
        listOf("skin", "camera").forEach { field ->
            val hostile = document("""{"byteLength":12}""").replace(
                """"meshes":""",
                """"nodes":[{"$field":0}],"meshes":""",
            )
            assertRejected(hostile, ByteArray(12))
        }
        listOf("animations", "skins", "cameras").forEach { field ->
            val malformed = document("""{"byteLength":12}""").replace(
                """"asset":{"version":"2.0"}""",
                """"asset":{"version":"2.0"},"$field":{}""",
            )
            assertRejected(malformed, ByteArray(12))
        }
    }

    @Test
    fun `validator accepts a bounded rest pose skin and rejects an out of range joint`() {
        val document = """
            {
              "asset":{"version":"2.0"},
              "buffers":[{"byteLength":160}],
              "bufferViews":[
                {"buffer":0,"byteLength":36},
                {"buffer":0,"byteOffset":36,"byteLength":12},
                {"buffer":0,"byteOffset":48,"byteLength":48},
                {"buffer":0,"byteOffset":96,"byteLength":64}
              ],
              "accessors":[
                {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
                {"bufferView":1,"componentType":5121,"count":3,"type":"VEC4"},
                {"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"},
                {"bufferView":3,"componentType":5126,"count":1,"type":"MAT4"}
              ],
              "meshes":[{"primitives":[{"attributes":{
                "POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2
              }}]}],
              "skins":[{"joints":[0],"inverseBindMatrices":3}],
              "nodes":[{}, {"skin":0,"children":[2]}, {"mesh":0}],
              "scenes":[{"nodes":[1]}]
            }
        """.trimIndent()
        val bytes = ByteArray(160)
        repeat(3) { vertex ->
            ByteBuffer.wrap(bytes, 48 + vertex * 16, 4)
                .order(ByteOrder.LITTLE_ENDIAN)
                .putFloat(1f)
        }
        repeat(4) { diagonal ->
            ByteBuffer.wrap(bytes, 96 + diagonal * 20, 4)
                .order(ByteOrder.LITTLE_ENDIAN)
                .putFloat(1f)
        }
        validateBytes(glb(document, bytes))

        bytes[36] = 1
        assertRejected(document, bytes)
    }

    @Test
    fun `validator rejects undeclared unknown and work-amplifying extensions`() {
        val nestedUndeclared = document("""{"byteLength":12}""").replace(
            """"mode":4""",
            """"mode":4,"extensions":{"UNKNOWN_nested":{}}""",
        )
        assertRejected(nestedUndeclared, ByteArray(12))

        listOf(
            "EXT_mesh_gpu_instancing",
            "KHR_draco_mesh_compression",
            "EXT_meshopt_compression",
            "KHR_texture_basisu",
            "KHR_lights_punctual",
        ).forEach { extension ->
            val hostile = document("""{"byteLength":12}""").replace(
                """"asset":{"version":"2.0"}""",
                """"asset":{"version":"2.0"},"extensionsUsed":["$extension"]""",
            )
            assertRejected(hostile, ByteArray(12))
        }
        val unusedDeclaration = document("""{"byteLength":12}""").replace(
            """"asset":{"version":"2.0"}""",
            """"asset":{"version":"2.0"},"extensionsUsed":["KHR_materials_unlit"]""",
        )
        assertRejected(unusedDeclaration, ByteArray(12))
        val requiredWithoutUsed = document("""{"byteLength":12}""").replace(
            """"asset":{"version":"2.0"}""",
            """"asset":{"version":"2.0"},"extensionsRequired":["KHR_materials_unlit"]""",
        )
        assertRejected(requiredWithoutUsed, ByteArray(12))
        val duplicateDeclaration = document("""{"byteLength":12}""").replace(
            """"asset":{"version":"2.0"}""",
            """"asset":{"version":"2.0"},
                "extensionsUsed":["KHR_materials_unlit","KHR_materials_unlit"]""",
        )
        assertRejected(duplicateDeclaration, ByteArray(12))
    }

    @Test
    fun `validator accepts declared non-amplifying material extension`() {
        val extended = document("""{"byteLength":36}""")
            .replace(
                """"asset":{"version":"2.0"}""",
                """"asset":{"version":"2.0","extras":{"extensions":{"ignored":{}}}},
                    "extensionsUsed":["KHR_materials_unlit"],
                    "extensionsRequired":["KHR_materials_unlit"],
                    "materials":[{"extensions":{"KHR_materials_unlit":{}}}]""",
            )
            .replace(""""mode":4""", """"mode":4,"material":0""")

        validateBytes(glb(extended, ByteArray(36)))
    }

    @Test
    fun `Android GLB limits match the bounded viewer contract`() {
        assertEquals(8 * 1024 * 1024, CREATION_GLB_MAXIMUM_JSON_BYTES)
        assertEquals(2_800_000, CREATION_GLB_MAXIMUM_DATA_URI_CHARACTERS)
        assertEquals(64, CREATION_GLB_MAXIMUM_BUFFERS)
        assertEquals(32_768, CREATION_GLB_MAXIMUM_BUFFER_VIEWS)
        assertEquals(16_384, CREATION_GLB_MAXIMUM_ACCESSORS)
        assertEquals(12_000_000L, CREATION_GLB_MAXIMUM_ACCESSOR_ELEMENTS)
        assertEquals(1_024, CREATION_GLB_MAXIMUM_MESHES)
        assertEquals(4_096, CREATION_GLB_MAXIMUM_PRIMITIVES)
        assertEquals(1_024, CREATION_GLB_MAXIMUM_MATERIALS)
        assertEquals(2_000_000L, CREATION_GLB_MAXIMUM_VERTICES)
        assertEquals(6_000_000L, CREATION_GLB_MAXIMUM_INDICES)
        assertEquals(256, CREATION_GLB_MAXIMUM_MORPH_TARGETS)
        assertEquals(8_000_000L, CREATION_GLB_MAXIMUM_MORPH_ELEMENTS)
        assertEquals(64, CREATION_GLB_MAXIMUM_SKINS)
        assertEquals(512, CREATION_GLB_MAXIMUM_JOINTS_PER_SKIN)
        assertEquals(4_096, CREATION_GLB_MAXIMUM_TOTAL_JOINTS)
        assertEquals(16, CREATION_GLB_MAXIMUM_PRIMITIVE_ATTRIBUTES)
        assertEquals(8, CREATION_GLB_MAXIMUM_MORPH_ATTRIBUTES)
        assertEquals(4_096, CREATION_GLB_MAXIMUM_NODES)
        assertEquals(64, CREATION_GLB_MAXIMUM_SCENES)
        assertEquals(256, CREATION_GLB_MAXIMUM_IMAGES)
        assertEquals(512, CREATION_GLB_MAXIMUM_TEXTURES)
        assertEquals(128, CREATION_GLB_MAXIMUM_SAMPLERS)
        assertEquals(8_192, CREATION_GLB_MAXIMUM_IMAGE_DIMENSION)
        assertEquals(16_777_216L, CREATION_GLB_MAXIMUM_IMAGE_PIXELS)
        assertEquals(33_554_432L, CREATION_GLB_MAXIMUM_DECODED_IMAGE_PIXELS)
        assertEquals(33_554_432L, CREATION_GLB_MAXIMUM_REFERENCED_TEXTURE_PIXELS)
    }

    @Test
    fun `validator rejects canonical metadata table overflows`() {
        val base = document("""{"byteLength":12}""")
        val buffer = """{"byteLength":12}"""
        val view = """{"buffer":0,"byteLength":12}"""
        val accessor =
            """{"bufferView":0,"componentType":5126,"count":1,"type":"VEC3"}"""
        val mesh = """{"primitives":[{"attributes":{"POSITION":0},"mode":4}]}"""
        val overflows = listOf(
            base.replace(
                """"buffers":[$buffer]""",
                """"buffers":[${repeatJson(CREATION_GLB_MAXIMUM_BUFFERS + 1, buffer)}]""",
            ),
            base.replace(
                """"bufferViews":[$view]""",
                """"bufferViews":[${repeatJson(CREATION_GLB_MAXIMUM_BUFFER_VIEWS + 1, view)}]""",
            ),
            base.replace(
                """"accessors":[$accessor]""",
                """"accessors":[${repeatJson(CREATION_GLB_MAXIMUM_ACCESSORS + 1, accessor)}]""",
            ),
            base.replace(
                """"meshes":[$mesh]""",
                """"meshes":[${repeatJson(CREATION_GLB_MAXIMUM_MESHES + 1, mesh)}]""",
            ),
            base.replace(
                """"asset":{"version":"2.0"}""",
                """"asset":{"version":"2.0"},
                    "materials":[${repeatJson(CREATION_GLB_MAXIMUM_MATERIALS + 1, "{}")}]""",
            ),
            base.replace(
                """"meshes":""",
                """"nodes":[${repeatJson(CREATION_GLB_MAXIMUM_NODES + 1, "{}")}],"meshes":""",
            ),
            base.replace(
                """"meshes":""",
                """"scenes":[${repeatJson(CREATION_GLB_MAXIMUM_SCENES + 1, "{}")}],"meshes":""",
            ),
            base.replace(
                """"meshes":""",
                """"images":[${repeatJson(CREATION_GLB_MAXIMUM_IMAGES + 1, "{}")}],"meshes":""",
            ),
            base.replace(
                """"meshes":""",
                """"textures":[${repeatJson(CREATION_GLB_MAXIMUM_TEXTURES + 1, "{}")}],"meshes":""",
            ),
            base.replace(
                """"meshes":""",
                """"samplers":[${repeatJson(CREATION_GLB_MAXIMUM_SAMPLERS + 1, "{}")}],"meshes":""",
            ),
        )

        overflows.forEach { assertRejected(it, ByteArray(12)) }
    }

    @Test
    fun `validator bounds morph targets and instantiated geometry`() {
        val target = """{"POSITION":0}"""
        val morphBomb = document("""{"byteLength":12}""").replace(
            """"mode":4""",
            """"mode":4,"targets":[
                ${repeatJson(CREATION_GLB_MAXIMUM_MORPH_TARGETS + 1, target)}
            ]""",
        )
        assertRejected(morphBomb, ByteArray(12))

        val nodes = repeatJson(2_001, """{"mesh":0}""")
        val instancingBomb = document("""{"byteLength":12000}""").replace(
            """"byteLength":12}""",
            """"byteLength":12000}""",
        ).replace(
            """"count":1,"type":"VEC3"""",
            """"count":1000,"type":"VEC3"""",
        ).replace(
            """"meshes":""",
            """"nodes":[$nodes],"meshes":""",
        )
        assertRejected(instancingBomb, ByteArray(12_000))
    }

    @Test
    fun `validator rejects texture dimension bombs before decode`() {
        val texture = ByteArray(24)
        byteArrayOf(
            0x89.toByte(), 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
        ).copyInto(texture)
        ByteBuffer.wrap(texture).order(ByteOrder.BIG_ENDIAN)
            .putInt(8, 13)
            .putInt(12, 0x49484452)
            .putInt(16, 32_768)
            .putInt(20, 32_768)
        val binary = ByteArray(60).also { texture.copyInto(it, 36) }
        val textured = """
            {
              "asset":{"version":"2.0"},
              "buffers":[{"byteLength":60}],
              "bufferViews":[
                {"buffer":0,"byteLength":36},
                {"buffer":0,"byteOffset":36,"byteLength":24}
              ],
              "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],
              "meshes":[{"primitives":[{"attributes":{"POSITION":0},"mode":4}]}],
              "nodes":[{"mesh":0}],
              "scenes":[{"nodes":[0]}],
              "images":[{"bufferView":1,"mimeType":"image/png"}],
              "textures":[{"source":0}]
            }
        """.trimIndent()

        assertRejected(textured, binary)
    }

    @Test
    fun `validator charges repeated texture references and rejects animated PNG`() {
        val texture = pngHeader(8_192, 2_048)
        val binary = ByteArray(36 + texture.size).also { texture.copyInto(it, 36) }
        assertRejected(
            texturedDocument(texture.size, """{"source":0},{"source":0},{"source":0}"""),
            binary,
        )

        val animated = pngHeader(1, 1).copyOf(33) + intsBigEndian(0, 0x6163544c, 0)
        val animatedBinary = ByteArray(36 + animated.size).also { animated.copyInto(it, 36) }
        assertRejected(
            texturedDocument(animated.size, """{"source":0}"""),
            animatedBinary,
        )
    }

    @Test
    fun `validator rejects deeply nested metadata before JSON parsing`() {
        val nested = buildString {
            repeat(129) { append('[') }
            append('0')
            repeat(129) { append(']') }
        }
        assertThrows(Exception::class.java) {
            validateCreationGlbJsonEnvelope(nested.encodeToByteArray())
        }
    }

    private fun document(buffer: String, bufferView: String? = null): String = """
        {
          "asset":{"version":"2.0"},
          "buffers":[$buffer],
          "bufferViews":[${bufferView ?: """{"buffer":0,"byteLength":36}"""}],
          "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],
          "meshes":[{"primitives":[{"attributes":{"POSITION":0},"mode":4}]}],
          "nodes":[{"mesh":0}],
          "scenes":[{"nodes":[0]}]
        }
    """.trimIndent()

    private fun texturedDocument(textureBytes: Int, textures: String): String = """
        {
          "asset":{"version":"2.0"},
          "buffers":[{"byteLength":${36 + textureBytes}}],
          "bufferViews":[
            {"buffer":0,"byteLength":36},
            {"buffer":0,"byteOffset":36,"byteLength":$textureBytes}
          ],
          "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],
          "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
          "nodes":[{"mesh":0}],
          "scenes":[{"nodes":[0]}],
          "images":[{"bufferView":1,"mimeType":"image/png"}],
          "textures":[$textures]
        }
    """.trimIndent()

    private fun pngHeader(width: Int, height: Int): ByteArray = ByteArray(24).also { bytes ->
        byteArrayOf(
            0x89.toByte(), 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
        ).copyInto(bytes)
        ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN)
            .putInt(8, 13)
            .putInt(12, 0x49484452)
            .putInt(16, width)
            .putInt(20, height)
    }

    private fun assertRejected(document: String, binary: ByteArray?) =
        assertRejectedBytes(glb(document, binary))

    private fun assertRejectedBytes(bytes: ByteArray) {
        assertThrows(Exception::class.java) { validateBytes(bytes) }
    }

    private fun validateBytes(bytes: ByteArray) {
        val directory = createTempDirectory("creation-glb-test").toFile()
        val file = File(directory, "model.glb").apply { writeBytes(bytes) }
        try {
            CreationArtifactValidator.validateGlb(file)
        } finally {
            file.delete()
            directory.delete()
        }
    }

    private fun glb(document: String, binary: ByteArray?): ByteArray {
        val json = document.encodeToByteArray().padToFour(0x20)
        val bin = binary?.padToFour(0)
        val total = 12 + 8 + json.size + if (bin == null) 0 else 8 + bin.size
        return ByteArrayOutputStream(total).use { output ->
            output.write(ints(0x46546C67, 2, total, json.size, 0x4E4F534A))
            output.write(json)
            if (bin != null) {
                output.write(ints(bin.size, 0x004E4942))
                output.write(bin)
            }
            output.toByteArray()
        }
    }

    private fun ByteArray.padToFour(padding: Int): ByteArray =
        copyOf(size + (4 - size % 4) % 4).also { bytes ->
            for (index in size until bytes.size) bytes[index] = padding.toByte()
        }

    private fun ints(vararg values: Int): ByteArray =
        ByteBuffer.allocate(values.size * 4).order(ByteOrder.LITTLE_ENDIAN)
            .apply { values.forEach(::putInt) }
            .array()

    private fun intsBigEndian(vararg values: Int): ByteArray =
        ByteBuffer.allocate(values.size * 4).order(ByteOrder.BIG_ENDIAN)
            .apply { values.forEach(::putInt) }
            .array()

    private fun repeatJson(count: Int, value: String): String =
        List(count) { value }.joinToString(",")
}

package dev.screengoated.toolbox.mobile.creation

import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.Base64
import kotlin.io.path.createTempDirectory
import org.json.JSONObject
import org.junit.Assert.assertThrows
import org.junit.Test

class CreationGlbParityHostileTest {
    @Test
    fun `asset version JSON padding and UTF-8 are exact`() {
        validate(baseDocument(), ByteArray(36))
        assertRejected(
            baseDocument().replace(""""version":"2.0"""", """"version":"2.future""""),
            ByteArray(36),
        )
        assertRejected(
            baseDocument().replace(
                """"version":"2.0"""",
                """"version":"2.0","minVersion":"2.1"""",
            ),
            ByteArray(36),
        )

        val json = baseDocument().encodeToByteArray()
        val padded = json.copyOf(json.size + (4 - json.size % 4) % 4 + 4)
        for (index in json.size until padded.size) padded[index] = 0x20
        padded[padded.lastIndex] = '\t'.code.toByte()
        assertRejected(glb(padded, ByteArray(36)))

        val malformed = json + byteArrayOf(0xff.toByte())
        val malformedPadded = malformed.copyOf(malformed.size + (4 - malformed.size % 4) % 4)
        for (index in malformed.size until malformedPadded.size) malformedPadded[index] = 0x20
        assertRejected(glb(malformedPadded, ByteArray(36)))
    }

    @Test
    fun `BIN ownership and URI MIME context fail closed`() {
        val emptyUri = baseDocument().replace(
            """{"byteLength":36}""",
            """{"byteLength":36,"uri":""}""",
        )
        assertRejected(emptyUri, ByteArray(36))

        val encoded = Base64.getEncoder().encodeToString(ByteArray(36))
        val embedded = baseDocument().replace(
            """{"byteLength":36}""",
            """{"byteLength":36,"uri":"data:application/octet-stream;base64,$encoded"}""",
        )
        validate(embedded, null)
        assertRejected(embedded, ByteArray(36))

        val emptyImageUri = baseDocument(extra = """,
            "images":[{"uri":"","bufferView":0,"mimeType":"image/png"}],
            "textures":[{"source":0}]
        """.trimIndent())
        assertRejected(emptyImageUri, ByteArray(36))

        val jpeg = byteArrayOf(
            0xff.toByte(), 0xd8.toByte(), 0xff.toByte(), 0xc0.toByte(),
            0, 7, 8, 0, 1, 0, 1,
        )
        val mismatched = baseDocument(extra = """,
            "images":[{"uri":"data:image/png;base64,${Base64.getEncoder().encodeToString(jpeg)}"}],
            "textures":[{"source":0}]
        """.trimIndent())
        assertRejected(mismatched, ByteArray(36))
    }

    @Test
    fun `accessor layout bounds and committed renderer floats are verified`() {
        val bounded = baseDocument(
            accessors = """[{
                "bufferView":0,"componentType":5126,"count":3,"type":"VEC3",
                "min":[-1,-1,-1],"max":[1,1,1]
            }]""".trimIndent(),
        )
        validate(bounded, ByteArray(36))
        assertRejected(
            bounded.replace(""""min":[-1,-1,-1]""", """"min":[2,-1,-1]"""),
            ByteArray(36),
        )
        val outside = ByteArray(36).also {
            ByteBuffer.wrap(it).order(ByteOrder.LITTLE_ENDIAN).putFloat(2f)
        }
        assertRejected(bounded, outside)
        val tolerated = ByteArray(36).also {
            ByteBuffer.wrap(it).order(ByteOrder.LITTLE_ENDIAN).putFloat(
                (1.0 + CREATION_GLB_POSITION_BOUNDS_ABSOLUTE_TOLERANCE / 2.0).toFloat(),
            )
        }
        validate(bounded, tolerated)
        val excessiveDrift = ByteArray(36).also {
            ByteBuffer.wrap(it).order(ByteOrder.LITTLE_ENDIAN).putFloat(
                (1.0 + CREATION_GLB_POSITION_BOUNDS_ABSOLUTE_TOLERANCE * 2.0).toFloat(),
            )
        }
        assertRejected(bounded, excessiveDrift)
        val notFinite = ByteArray(36).also {
            ByteBuffer.wrap(it).order(ByteOrder.LITTLE_ENDIAN).putFloat(Float.NaN)
        }
        assertRejected(baseDocument(), notFinite)

        val morph = baseDocument(
            buffer = """{"byteLength":72}""",
            views = """[
                {"buffer":0,"byteLength":36},
                {"buffer":0,"byteOffset":36,"byteLength":36}
            ]""".trimIndent(),
            accessors = """[
                {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
                {
                    "bufferView":1,"componentType":5126,"count":3,"type":"VEC3",
                    "min":[-1,-1,-1],"max":[1,1,1]
                }
            ]""".trimIndent(),
            primitiveExtra = ""","targets":[{"POSITION":1}]""",
        )
        val morphBytes = ByteArray(72).also {
            ByteBuffer.wrap(it).order(ByteOrder.LITTLE_ENDIAN).putFloat(36, 2f)
        }
        assertRejected(morph, morphBytes)

        val interleaved = baseDocument(
            buffer = """{"byteLength":48}""",
            views = """[{"buffer":0,"byteLength":48,"byteStride":16}]""",
            accessors = """[{
                "bufferView":0,"byteOffset":12,"componentType":5126,"count":3,"type":"VEC3"
            }]""",
        )
        assertRejected(interleaved, ByteArray(48))

        val misalignedColor = baseDocument(
            buffer = """{"byteLength":62}""",
            views = """[
                {"buffer":0,"byteLength":36},
                {"buffer":0,"byteOffset":38,"byteLength":24}
            ]""".trimIndent(),
            accessors = """[
                {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
                {
                    "bufferView":1,"componentType":5123,"normalized":true,
                    "count":3,"type":"VEC4"
                }
            ]""".trimIndent(),
            attributes = """{"POSITION":0,"COLOR_0":1}""",
        )
        assertRejected(misalignedColor, ByteArray(64))
    }

    @Test
    fun `triangle scene transform and material values are bounded`() {
        val fourVertices = baseDocument(
            buffer = """{"byteLength":48}""",
            views = """[{"buffer":0,"byteLength":48}]""",
            accessors = """[{
                "bufferView":0,"componentType":5126,"count":4,"type":"VEC3"
            }]""",
        )
        assertRejected(fourVertices, ByteArray(48))

        val repeatedRoot = baseDocument(
            scenes = """[{"nodes":[0]},{"nodes":[0]}]""",
        )
        assertRejected(repeatedRoot, ByteArray(36))

        val excessiveTransform = baseDocument(
            nodes = """[{"mesh":0,"translation":[20000000,0,0]}]""",
        )
        assertRejected(excessiveTransform, ByteArray(36))

        val material = JSONObject("""{"metallicFactor":20000000}""")
        assertThrows(Exception::class.java) {
            validateGlbMaterialNumericValues(material)
        }
        assertThrows(Exception::class.java) {
            validateGlbMaterialNumericValues(
                JSONObject("""{"pbrMetallicRoughness":{"metallicFactor":"1e300"}}"""),
            )
        }
        validateGlbMaterialNumericValues(
            JSONObject(
                """{
                    "name":"bounded","alphaMode":"OPAQUE","doubleSided":true,
                    "pbrMetallicRoughness":{"baseColorFactor":[1,1,1,1]},
                    "emissiveFactor":[0,0,0],
                    "normalTexture":{"scale":1,"extensions":{"KHR_texture_transform":{
                        "offset":[0,0],"scale":[1,1]
                    }}},
                    "extensions":{
                        "KHR_materials_sheen":{"sheenColorFactor":[1,1,1]},
                        "KHR_materials_specular":{"specularColorFactor":[1,1,1]},
                        "KHR_materials_volume":{"attenuationColor":[1,1,1]}
                    }
                }""",
            ),
        )
        listOf(
            """{"name":1}""",
            """{"alphaMode":1}""",
            """{"doubleSided":1}""",
            """{"pbrMetallicRoughness":{"baseColorFactor":[1,1,"1",1]}}""",
        ).forEach { hostile ->
            assertThrows(Exception::class.java) {
                validateGlbMaterialNumericValues(JSONObject(hostile))
            }
        }
    }

    @Test
    fun `texture clone accounting matches the shipped loader`() {
        val pixels = CREATION_GLB_MAXIMUM_IMAGE_PIXELS
        val equalChannel = JSONObject(
            """{"materials":[{"normalTexture":{
                "index":0,"texCoord":1,
                "extensions":{"KHR_texture_transform":{"texCoord":1}}
            }}]}""",
        )
        validateGlbMaterialTextures(equalChannel, listOf(pixels), pixels)

        val twoClones = JSONObject(
            """{"materials":[{"normalTexture":{
                "index":0,"texCoord":1,
                "extensions":{"KHR_texture_transform":{"offset":[0,0]}}
            }}]}""",
        )
        assertThrows(Exception::class.java) {
            validateGlbMaterialTextures(twoClones, listOf(pixels), pixels)
        }
    }

    @Test
    fun `sampler enums and types are strict`() {
        validateGlbSampler(
            JSONObject(
                """{"name":"nearest","magFilter":9728,"minFilter":9987,
                    "wrapS":33071,"wrapT":10497}""",
            ),
        )
        listOf(
            """{"magFilter":"9728"}""",
            """{"minFilter":9999}""",
            """{"wrapS":1}""",
            """{"name":1}""",
        ).forEach { hostile ->
            assertThrows(Exception::class.java) {
                validateGlbSampler(JSONObject(hostile))
            }
        }
    }

    private fun baseDocument(
        buffer: String = """{"byteLength":36}""",
        views: String = """[{"buffer":0,"byteLength":36}]""",
        accessors: String =
            """[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}]""",
        attributes: String = """{"POSITION":0}""",
        primitiveExtra: String = "",
        nodes: String = """[{"mesh":0}]""",
        scenes: String = """[{"nodes":[0]}]""",
        extra: String = "",
    ): String = """
        {
          "asset":{"version":"2.0"},
          "buffers":[$buffer],
          "bufferViews":$views,
          "accessors":$accessors,
          "meshes":[{"primitives":[{"attributes":$attributes$primitiveExtra}]}],
          "nodes":$nodes,
          "scenes":$scenes
          $extra
        }
    """.trimIndent()

    private fun assertRejected(document: String, binary: ByteArray?) {
        assertRejected(glb(document.encodeToByteArray().spacePad(), binary))
    }

    private fun assertRejected(bytes: ByteArray) {
        assertThrows(Exception::class.java) { validate(bytes) }
    }

    private fun validate(document: String, binary: ByteArray?) {
        validate(glb(document.encodeToByteArray().spacePad(), binary))
    }

    private fun validate(bytes: ByteArray) {
        val directory = createTempDirectory("creation-glb-hostile").toFile()
        val file = File(directory, "model.glb").apply { writeBytes(bytes) }
        try {
            CreationArtifactValidator.validateGlb(file)
        } finally {
            file.delete()
            directory.delete()
        }
    }

    private fun glb(json: ByteArray, binary: ByteArray?): ByteArray {
        val bin = binary?.zeroPad()
        val total = 12 + 8 + json.size + if (bin == null) 0 else 8 + bin.size
        return ByteArrayOutputStream(total).use { output ->
            output.write(ints(0x46546c67, 2, total, json.size, 0x4e4f534a))
            output.write(json)
            if (bin != null) {
                output.write(ints(bin.size, 0x004e4942))
                output.write(bin)
            }
            output.toByteArray()
        }
    }

    private fun ByteArray.spacePad(): ByteArray =
        copyOf(size + (4 - size % 4) % 4).also { result ->
            for (index in size until result.size) result[index] = 0x20
        }

    private fun ByteArray.zeroPad(): ByteArray = copyOf(size + (4 - size % 4) % 4)

    private fun ints(vararg values: Int): ByteArray =
        ByteBuffer.allocate(values.size * Int.SIZE_BYTES)
            .order(ByteOrder.LITTLE_ENDIAN)
            .apply { values.forEach(::putInt) }
            .array()
}

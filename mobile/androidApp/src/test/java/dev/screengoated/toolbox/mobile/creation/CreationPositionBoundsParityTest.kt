package dev.screengoated.toolbox.mobile.creation

import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.io.path.createTempDirectory
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

/**
 * Replays `parity-fixtures/image-to-3d/position-bounds.json` through the
 * production GLB validator, as Windows does in asset_position_bounds_tests.rs:
 * stale POSITION bounds are rejected and recomputed bounds pass.
 */
class CreationPositionBoundsParityTest {
    private val fixture: JSONObject = JSONObject(
        generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) { it.parentFile }
            .map { File(it, "parity-fixtures/image-to-3d/position-bounds.json") }
            .first(File::isFile)
            .readText(),
    )

    @Test
    fun sharedPositionBoundsRejectStaleAndAcceptRecomputedExtents() {
        val positions = fixture.getJSONArray("positions").let { rows ->
            (0 until rows.length()).map { vector(rows.getJSONArray(it)) }
        }
        val minimum = vector(fixture.getJSONArray("minimum"))
        val maximum = vector(fixture.getJSONArray("maximum"))
        for (axis in 0 until 3) {
            assertEquals(positions.minOf { it[axis] }, minimum[axis], 0.0)
            assertEquals(positions.maxOf { it[axis] }, maximum[axis], 0.0)
        }

        // A triangle needs three vertices; the extra one repeats the first.
        val binary = ByteBuffer.allocate(36).order(ByteOrder.LITTLE_ENDIAN).apply {
            (positions + listOf(positions.first())).take(3).forEach { position ->
                position.forEach { putFloat(it.toFloat()) }
            }
        }.array()

        validate(triangle(fixture.getJSONArray("minimum"), fixture.getJSONArray("maximum")), binary)
        val stale = assertThrows(IllegalArgumentException::class.java) {
            validate(triangle(fixture.getJSONArray("staleMinimum"), fixture.getJSONArray("staleMaximum")), binary)
        }
        assertEquals("The model result exceeds its declared position bounds", stale.message)
    }

    private fun vector(values: JSONArray): List<Double> = (0 until 3).map(values::getDouble)

    private fun triangle(minimum: JSONArray, maximum: JSONArray): String = """
        {
          "asset":{"version":"2.0"},
          "buffers":[{"byteLength":36}],
          "bufferViews":[{"buffer":0,"byteLength":36}],
          "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3",
            "min":$minimum,"max":$maximum}],
          "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
          "nodes":[{"mesh":0}],
          "scenes":[{"nodes":[0]}]
        }
    """.trimIndent()

    private fun validate(document: String, binary: ByteArray) {
        val json = document.encodeToByteArray().let { raw ->
            raw.copyOf(raw.size + (4 - raw.size % 4) % 4).also { padded ->
                for (index in raw.size until padded.size) padded[index] = 0x20
            }
        }
        val total = 12 + 8 + json.size + 8 + binary.size
        val bytes = ByteArrayOutputStream(total).use { output ->
            output.write(ints(0x46546c67, 2, total, json.size, 0x4e4f534a))
            output.write(json)
            output.write(ints(binary.size, 0x004e4942))
            output.write(binary)
            output.toByteArray()
        }
        val directory = createTempDirectory("creation-position-bounds").toFile()
        val file = File(directory, "model.glb").apply { writeBytes(bytes) }
        try {
            CreationArtifactValidator.validateGlb(file)
        } finally {
            file.delete()
            directory.delete()
        }
    }

    private fun ints(vararg values: Int): ByteArray =
        ByteBuffer.allocate(values.size * Int.SIZE_BYTES)
            .order(ByteOrder.LITTLE_ENDIAN)
            .apply { values.forEach(::putInt) }
            .array()
}

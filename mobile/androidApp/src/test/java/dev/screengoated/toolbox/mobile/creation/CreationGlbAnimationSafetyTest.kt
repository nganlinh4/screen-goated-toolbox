package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class CreationGlbAnimationSafetyTest {
    @Test
    fun `shared animation cases match Windows validation`() {
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) { it.parentFile }
            .first { File(it, "parity-fixtures").isDirectory }
        val fixture = JSONObject(File(root, "parity-fixtures/image-to-3d/animation-contract.json").readText())
        val cases = fixture.getJSONArray("cases")
        repeat(cases.length()) { index ->
            val case = cases.getJSONObject(index)
            val times = case.getJSONArray("times")
            val values = case.getJSONArray("values")
            val bytes = ByteBuffer.allocate((times.length() + values.length()) * 4).order(ByteOrder.LITTLE_ENDIAN)
            repeat(times.length()) { bytes.putFloat(times.getDouble(it).toFloat()) }
            repeat(values.length()) { bytes.putFloat(values.getDouble(it).toFloat()) }
            val document = JSONObject("""{
                "nodes":[{}],"animations":[{
                "samplers":[{"input":0,"output":1,"interpolation":${JSONObject.quote(case.getString("interpolation"))}}],
                "channels":[{"sampler":0,"target":{"node":0,"path":"translation"}}]}]
            }""")
            val accessors = listOf(
                accessor(times.length(), 1, 0),
                accessor(values.length() / 3, 3, times.length().toLong() * 4),
            )
            val file = File.createTempFile("animation-validation-", ".bin")
            try {
                val valid = runCatching {
                    RandomAccessFile(file, "r").use { input ->
                        validateGlbAnimations(
                            document, accessors, input,
                            listOf(GlbBuffer(bytes.capacity().toLong(), null, bytes.array())),
                            listOf(GlbBufferView(0, 0, bytes.capacity().toLong(), 0)),
                        )
                    }
                }.isSuccess
                assertEquals(case.getString("name"), case.getBoolean("valid"), valid)
            } finally { check(file.delete()) }
        }
    }

    private fun accessor(count: Int, width: Int, offset: Long) = GlbAccessor(
        type = if (width == 1) "SCALAR" else "VEC3", componentType = GLB_FLOAT,
        componentCount = width, count = count.toLong(), view = 0, offset = offset,
        absoluteOffset = offset, elementBytes = width * 4, stride = 0,
        normalized = false, minimum = null, maximum = null,
    )
}

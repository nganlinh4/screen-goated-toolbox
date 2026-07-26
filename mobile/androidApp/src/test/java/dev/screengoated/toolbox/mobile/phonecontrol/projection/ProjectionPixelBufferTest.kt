package dev.screengoated.toolbox.mobile.phonecontrol.projection

import java.nio.ByteBuffer
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

class ProjectionPixelBufferTest {
    @Test
    fun `copies visible rows when final row padding is absent`() {
        val source = ByteBuffer.wrap(
            byteArrayOf(
                1, 2, 3, 4,
                5, 6, 7, 8,
                99, 99, 99, 99,
                9, 10, 11, 12,
                13, 14, 15, 16,
            ),
        )

        val copied = copyVisibleRgbaBytes(source, 2, 2, 4, 12)
        val actual = ByteArray(copied.remaining()).also(copied::get)

        assertArrayEquals((1..16).map { it.toByte() }.toByteArray(), actual)
    }

    @Test
    fun `drops bytes between adjacent pixels`() {
        val source = ByteBuffer.wrap(
            byteArrayOf(
                1, 2, 3, 4, 88, 88,
                5, 6, 7, 8, 77, 77,
            ),
        )

        val copied = copyVisibleRgbaBytes(source, 2, 1, 6, 12)
        val actual = ByteArray(copied.remaining()).also(copied::get)

        assertArrayEquals(byteArrayOf(1, 2, 3, 4, 5, 6, 7, 8), actual)
    }

    @Test(expected = IllegalArgumentException::class)
    fun `rejects a buffer missing a visible pixel`() {
        copyVisibleRgbaBytes(ByteBuffer.allocate(15), 2, 2, 4, 8)
    }

    @Test
    fun `preserves the caller buffer position`() {
        val source = ByteBuffer.allocate(12).apply {
            position(4)
            put(byteArrayOf(1, 2, 3, 4))
            position(4)
        }

        copyVisibleRgbaBytes(source, 1, 1, 4, 4)

        assertEquals(4, source.position())
    }
}

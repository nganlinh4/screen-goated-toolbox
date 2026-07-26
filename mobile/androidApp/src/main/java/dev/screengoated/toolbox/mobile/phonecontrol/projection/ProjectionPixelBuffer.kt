package dev.screengoated.toolbox.mobile.phonecontrol.projection

import java.nio.ByteBuffer

internal fun copyVisibleRgbaBytes(
    source: ByteBuffer,
    width: Int,
    height: Int,
    pixelStride: Int,
    rowStride: Int,
): ByteBuffer {
    require(width > 0 && height > 0) { "Projection dimensions must be positive" }
    require(pixelStride >= RGBA_BYTES_PER_PIXEL) {
        "Projection pixel stride cannot contain RGBA pixels"
    }
    require(rowStride >= width * pixelStride) {
        "Projection row stride is smaller than the visible row"
    }
    val outputBytes = width.toLong() * height * RGBA_BYTES_PER_PIXEL
    require(outputBytes <= Int.MAX_VALUE) { "Projection frame is too large" }

    val input = source.duplicate()
    val base = input.position()
    val visibleEndExclusive = base.toLong() +
        (height - 1L) * rowStride +
        (width - 1L) * pixelStride +
        RGBA_BYTES_PER_PIXEL
    require(visibleEndExclusive <= input.limit()) {
        "Projection buffer does not contain every visible pixel"
    }

    val output = ByteBuffer.allocateDirect(outputBytes.toInt())
    repeat(height) { row ->
        val rowStart = base + row * rowStride
        if (pixelStride == RGBA_BYTES_PER_PIXEL) {
            val visibleRow = input.duplicate()
            visibleRow.position(rowStart)
            visibleRow.limit(rowStart + width * RGBA_BYTES_PER_PIXEL)
            output.put(visibleRow)
        } else {
            repeat(width) { column ->
                val pixelStart = rowStart + column * pixelStride
                repeat(RGBA_BYTES_PER_PIXEL) { channel ->
                    output.put(input.get(pixelStart + channel))
                }
            }
        }
    }
    output.flip()
    return output
}

private const val RGBA_BYTES_PER_PIXEL = 4

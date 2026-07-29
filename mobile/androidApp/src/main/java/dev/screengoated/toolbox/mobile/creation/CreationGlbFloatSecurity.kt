package dev.screengoated.toolbox.mobile.creation

import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.abs

internal class CreationGlbFloatScanner(
    private val input: RandomAccessFile,
    private val buffers: List<GlbBuffer>,
    private val views: List<GlbBufferView>,
) {
    fun validate(accessor: GlbAccessor, containDeclaredBounds: Boolean) {
        require(accessor.componentType == GLB_FLOAT && accessor.view in views.indices) {
            "The model result has invalid renderer data"
        }
        val view = views[accessor.view]
        val buffer = buffers[view.buffer]
        val reader = FloatReader(input, buffer)
        val stride = if (accessor.stride == 0) accessor.elementBytes else accessor.stride
        repeat(accessor.count.toInt()) { element ->
            repeat(accessor.componentCount) { component ->
                val offset = checkedAdd(
                    accessor.absoluteOffset,
                    checkedAdd(
                        checkedMultiply(element.toLong(), stride.toLong()),
                        component.toLong() * Float.SIZE_BYTES,
                    ),
                )
                val value = reader.read(offset)
                require(value.isFinite() && abs(value) <= CREATION_GLB_MAXIMUM_ABSOLUTE_RENDERER_VALUE) {
                    "The model result contains invalid renderer values"
                }
                if (containDeclaredBounds && accessor.minimum != null && accessor.maximum != null) {
                    require(
                        positionBoundContains(
                            value,
                            accessor.minimum[component],
                            accessor.maximum[component],
                        )
                    ) { "The model result exceeds its declared position bounds" }
                }
            }
        }
    }
}

private fun positionBoundContains(value: Double, minimum: Double, maximum: Double): Boolean {
    val minimumTolerance = positionBoundTolerance(value, minimum)
    val maximumTolerance = positionBoundTolerance(value, maximum)
    return value >= minimum - minimumTolerance && value <= maximum + maximumTolerance
}

private fun positionBoundTolerance(value: Double, bound: Double): Double =
    maxOf(
        CREATION_GLB_POSITION_BOUNDS_ABSOLUTE_TOLERANCE,
        maxOf(abs(value), abs(bound)) * CREATION_GLB_POSITION_BOUNDS_RELATIVE_TOLERANCE,
    )

private class FloatReader(
    private val input: RandomAccessFile,
    private val buffer: GlbBuffer,
) {
    private val page = ByteArray(FLOAT_PAGE_BYTES)
    private var pageStart = -1L
    private var pageLength = 0

    fun read(offset: Long): Double {
        require(offset >= 0 && checkedAdd(offset, Float.SIZE_BYTES.toLong()) <= buffer.length) {
            "The model result renderer data exceeds its buffer"
        }
        buffer.embedded?.let { embedded ->
            return littleEndianFloat(embedded, offset.toInt())
        }
        if (
            offset < pageStart ||
            checkedAdd(offset, Float.SIZE_BYTES.toLong()) > pageStart + pageLength
        ) {
            val binary = requireNotNull(buffer.binary)
            pageStart = offset
            pageLength = minOf(page.size.toLong(), buffer.length - offset).toInt()
            input.seek(checkedAdd(binary.offset, offset))
            input.readFully(page, 0, pageLength)
        }
        return littleEndianFloat(page, (offset - pageStart).toInt())
    }
}

private fun littleEndianFloat(bytes: ByteArray, offset: Int): Double =
    ByteBuffer.wrap(bytes, offset, Float.SIZE_BYTES)
        .order(ByteOrder.LITTLE_ENDIAN)
        .float
        .toDouble()

internal const val CREATION_GLB_POSITION_BOUNDS_ABSOLUTE_TOLERANCE = 1.0 / 32_768.0
internal const val CREATION_GLB_POSITION_BOUNDS_RELATIVE_TOLERANCE =
    4.0 * 1.1920928955078125e-7
private const val FLOAT_PAGE_BYTES = 64 * 1024

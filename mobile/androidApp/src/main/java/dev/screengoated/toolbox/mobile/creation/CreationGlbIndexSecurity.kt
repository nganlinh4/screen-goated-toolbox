package dev.screengoated.toolbox.mobile.creation

import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder

internal fun validateCreationGlbPrimitiveCount(count: Long, mode: Int) {
    require(mode == 4 && count >= 3 && count % 3L == 0L) {
        "The model result has invalid triangle geometry"
    }
}

internal fun validateCreationGlbPrimitiveIndices(
    input: RandomAccessFile,
    buffers: List<GlbBuffer>,
    views: List<GlbBufferView>,
    accessor: GlbAccessor,
    positionCount: Long,
    mode: Int,
) {
    validateCreationGlbPrimitiveCount(accessor.count, mode)
    require(accessor.view in views.indices) {
        "The model result has invalid primitive indices"
    }
    val view = views[accessor.view]
    val componentBytes = when (accessor.componentType) {
        GLB_UNSIGNED_BYTE -> 1
        GLB_UNSIGNED_SHORT -> 2
        GLB_UNSIGNED_INT -> 4
        else -> error("The model result has invalid primitive indices")
    }
    val stride = if (view.stride == 0) accessor.elementBytes else view.stride
    val reader = GlbIndexReader(input, buffers[view.buffer])
    repeat(accessor.count.toInt()) { index ->
        val logicalOffset = glbIndexCheckedAdd(
            view.offset,
            glbIndexCheckedAdd(accessor.offset, glbIndexCheckedMultiply(index.toLong(), stride.toLong())),
        )
        val value = reader.unsigned(logicalOffset, componentBytes)
        require(value < positionCount) { "The model result index exceeds its position accessor" }
    }
}

private class GlbIndexReader(
    private val input: RandomAccessFile,
    private val buffer: GlbBuffer,
) {
    private val page = ByteArray(GLB_INDEX_PAGE_BYTES)
    private var pageStart = -1L
    private var pageLength = 0

    fun unsigned(offset: Long, bytes: Int): Long {
        require(offset >= 0 && glbIndexCheckedAdd(offset, bytes.toLong()) <= buffer.length) {
            "The model result indices exceed their buffer"
        }
        buffer.embedded?.let { embedded ->
            return unsignedValue(embedded, offset.toInt(), bytes)
        }
        if (offset < pageStart || glbIndexCheckedAdd(offset, bytes.toLong()) > pageStart + pageLength) {
            val binary = requireNotNull(buffer.binary)
            pageStart = offset
            pageLength = minOf(page.size.toLong(), buffer.length - offset).toInt()
            input.seek(glbIndexCheckedAdd(binary.offset, offset))
            input.readFully(page, 0, pageLength)
        }
        return unsignedValue(page, (offset - pageStart).toInt(), bytes)
    }
}

private fun unsignedValue(bytes: ByteArray, offset: Int, size: Int): Long {
    val buffer = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
    return when (size) {
        1 -> (bytes[offset].toInt() and 0xff).toLong()
        2 -> (buffer.getShort(offset).toInt() and 0xffff).toLong()
        4 -> buffer.getInt(offset).toLong() and 0xffff_ffffL
        else -> error("The model result has invalid primitive indices")
    }
}

private fun glbIndexCheckedAdd(left: Long, right: Long): Long =
    runCatching { Math.addExact(left, right) }
        .getOrElse { error("The model result index metadata is too large") }

private fun glbIndexCheckedMultiply(left: Long, right: Long): Long =
    runCatching { Math.multiplyExact(left, right) }
        .getOrElse { error("The model result index metadata is too large") }

private const val GLB_INDEX_PAGE_BYTES = 64 * 1024
private const val GLB_UNSIGNED_BYTE = 5_121
private const val GLB_UNSIGNED_SHORT = 5_123
private const val GLB_UNSIGNED_INT = 5_125

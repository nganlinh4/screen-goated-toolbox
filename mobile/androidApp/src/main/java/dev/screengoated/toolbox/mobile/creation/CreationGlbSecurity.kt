package dev.screengoated.toolbox.mobile.creation

import java.io.RandomAccessFile
import java.util.Base64
import org.json.JSONArray
import org.json.JSONObject

internal data class CreationGlbBinaryChunk(val offset: Long, val length: Long)

internal fun validateCreationGlbJsonEnvelope(bytes: ByteArray) {
    require(bytes.size in 1..CREATION_GLB_MAXIMUM_JSON_BYTES) {
        "The model result metadata is too large"
    }
    var depth = 0
    var inString = false
    var escaped = false
    bytes.forEach { byte ->
        val character = byte.toInt().toChar()
        if (inString) {
            if (escaped) {
                escaped = false
            } else if (character == '\\') {
                escaped = true
            } else if (character == '"') {
                inString = false
            }
        } else {
            when (character) {
                '"' -> inString = true
                '{', '[' -> {
                    depth += 1
                    require(depth <= MAXIMUM_GLB_JSON_DEPTH) {
                        "The model result metadata is too complex"
                    }
                }
                '}', ']' -> {
                    depth -= 1
                    require(depth >= 0) { "The model result is invalid" }
                }
            }
        }
    }
    require(!inString && !escaped && depth == 0) { "The model result is invalid" }
}

internal fun validateCreationGlbDocument(
    document: JSONObject,
    input: RandomAccessFile,
    binaryChunk: CreationGlbBinaryChunk?,
) {
    validateCreationGlbExtensions(document)
    val buffers = validateGlbBuffers(document, input, binaryChunk)
    val views = validateGlbBufferViews(document, buffers)
    val accessors = validateGlbAccessors(document, views)
    val meshCosts = validateGlbMeshes(document, accessors, input, buffers, views)
    validateGlbSceneGraph(document, meshCosts)
    validateGlbUnsupportedWork(document)
    validateGlbImagesAndTextures(document, input, buffers, views)
}

private fun validateGlbBuffers(
    document: JSONObject,
    input: RandomAccessFile,
    binaryChunk: CreationGlbBinaryChunk?,
): List<GlbBuffer> {
    val values = document.requiredArray("buffers", 1, MAXIMUM_GLB_BUFFERS)
    val buffers = List(values.length()) { index ->
        val value = values.requiredObject(index)
        val length = value.requiredLong("byteLength", 1, CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES)
        val hasUri = value.has("uri")
        val uri = if (hasUri) {
            requireNotNull(value.opt("uri") as? String) {
                "The model result has an invalid buffer"
            }
        } else {
            ""
        }
        if (!hasUri) {
            require(
                index == 0 &&
                    binaryChunk != null
            ) {
                "The model result has an incomplete buffer"
            }
            validateGlbBinaryAlignmentPadding(input, binaryChunk, length)
            GlbBuffer(length, binaryChunk, null)
        } else {
            val bytes = decodeGlbDataUri(
                uri,
                setOf("data:application/octet-stream;base64,"),
            )
            require(bytes.size.toLong() == length) {
                "The model result buffer length is invalid"
            }
            GlbBuffer(length, null, bytes)
        }
    }
    require((binaryChunk != null) == (buffers.first().binary != null)) {
        "The model result has an unexpected binary chunk"
    }
    return buffers
}

private fun validateGlbBinaryAlignmentPadding(
    input: RandomAccessFile,
    chunk: CreationGlbBinaryChunk,
    logicalLength: Long,
) {
    val paddingLength = (4L - logicalLength % 4L) % 4L
    require(chunk.length == checkedAdd(logicalLength, paddingLength)) {
        "The model result buffer length is invalid"
    }
    if (paddingLength == 0L) return
    val originalPosition = input.filePointer
    try {
        input.seek(checkedAdd(chunk.offset, logicalLength))
        val padding = ByteArray(paddingLength.toInt()).also(input::readFully)
        require(padding.all { it == 0.toByte() }) {
            "The model result buffer padding is invalid"
        }
    } finally {
        input.seek(originalPosition)
    }
}

private fun validateGlbBufferViews(
    document: JSONObject,
    buffers: List<GlbBuffer>,
): List<GlbBufferView> {
    val values = document.optionalArray("bufferViews")
    require(values.length() <= MAXIMUM_GLB_BUFFER_VIEWS) {
        "The model result metadata is too complex"
    }
    var aggregateBytes = 0L
    return List(values.length()) { index ->
        val value = values.requiredObject(index)
        val bufferIndex = value.requiredIndex("buffer", buffers.size)
        val offset = value.optionalLong("byteOffset", 0, buffers[bufferIndex].length)
        val length = value.requiredLong("byteLength", 1, buffers[bufferIndex].length)
        require(checkedAdd(offset, length) <= buffers[bufferIndex].length) {
            "The model result has an invalid buffer view"
        }
        aggregateBytes = checkedAdd(aggregateBytes, length)
        require(aggregateBytes <= CREATION_GLB_MAXIMUM_AGGREGATE_BUFFER_VIEW_BYTES) {
            "The model result contains too much buffer-view data"
        }
        val stride = value.optionalLong("byteStride", 0, MAXIMUM_GLB_BYTE_STRIDE.toLong()).toInt()
        require(stride == 0 || stride >= 4 && stride % 4 == 0) {
            "The model result has an invalid buffer stride"
        }
        if (value.has("target")) {
            require(value.requiredLong("target", 0, Int.MAX_VALUE.toLong()).toInt() in GLB_BUFFER_TARGETS) {
                "The model result has an invalid buffer target"
            }
        }
        GlbBufferView(bufferIndex, offset, length, stride)
    }
}

private fun validateGlbAccessors(
    document: JSONObject,
    views: List<GlbBufferView>,
): List<GlbAccessor> {
    val values = document.requiredArray("accessors", 1, MAXIMUM_GLB_ACCESSORS)
    var aggregateElements = 0L
    return List(values.length()) { index ->
        val value = values.requiredObject(index)
        require(!value.has("sparse")) { "Sparse model geometry is unsupported" }
        val componentType = value.requiredLong("componentType", 0, Int.MAX_VALUE.toLong()).toInt()
        val componentBytes = GLB_COMPONENT_BYTES[componentType]
        requireNotNull(componentBytes) { "The model result has an invalid accessor component" }
        val type = value.optString("type")
        val componentCount = GLB_TYPE_COMPONENTS[type]
        requireNotNull(componentCount) { "The model result has an invalid accessor type" }
        val count = value.requiredLong("count", 1, MAXIMUM_GLB_ACCESSOR_COUNT)
        aggregateElements = checkedAdd(aggregateElements, count)
        require(aggregateElements <= MAXIMUM_GLB_ACCESSOR_ELEMENTS) {
            "The model result geometry is too complex"
        }
        val elementBytes = glbElementBytes(type, componentBytes)
        val viewIndex = value.requiredIndex("bufferView", views.size)
        val offset = value.optionalLong("byteOffset", 0, CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES)
        val absoluteOffset = checkedAdd(views[viewIndex].offset, offset)
        require(offset % componentBytes == 0L && absoluteOffset % componentBytes == 0L) {
            "The model result has an invalid accessor offset"
        }
        val normalized = if (value.has("normalized")) {
            require(value.opt("normalized") is Boolean) {
                "The model result has invalid accessor metadata"
            }
            value.getBoolean("normalized")
        } else {
            false
        }
        require(!normalized || componentType !in setOf(5_125, GLB_FLOAT)) {
            "The model result has invalid accessor normalization"
        }
        val bounds = validateGlbAccessorBounds(value, componentType, componentCount)
        validateAccessorRange(views[viewIndex], offset, count, elementBytes, componentBytes)
        GlbAccessor(
            type = type,
            componentType = componentType,
            componentCount = componentCount,
            count = count,
            view = viewIndex,
            offset = offset,
            absoluteOffset = absoluteOffset,
            elementBytes = elementBytes,
            stride = views[viewIndex].stride,
            normalized = normalized,
            minimum = bounds?.first,
            maximum = bounds?.second,
        )
    }
}

private fun validateAccessorRange(
    view: GlbBufferView,
    offset: Long,
    count: Long,
    elementBytes: Int,
    componentBytes: Int,
) {
    val stride = if (view.stride == 0) elementBytes else view.stride
    require(
        stride >= elementBytes &&
            stride <= MAXIMUM_GLB_BYTE_STRIDE &&
            stride % componentBytes == 0
    ) { "The model result has an invalid accessor stride" }
    val end = if (view.stride == 0) {
        checkedAdd(offset, checkedMultiply(count, elementBytes.toLong()))
    } else {
        require(offset % stride + elementBytes <= stride) {
            "The model result has an invalid interleaved accessor"
        }
        val allocationStart = checkedMultiply(offset / stride, stride.toLong())
        checkedAdd(allocationStart, checkedMultiply(count, stride.toLong()))
    }
    require(end <= view.length) { "The model result accessor exceeds its buffer view" }
}

private fun validateGlbAccessorBounds(
    value: JSONObject,
    componentType: Int,
    componentCount: Int,
): Pair<DoubleArray, DoubleArray>? {
    if (!value.has("min") && !value.has("max")) return null
    require(value.has("min") && value.has("max")) {
        "The model result has incomplete accessor bounds"
    }
    val minimum = value.requiredFiniteArray("min", componentCount)
    val maximum = value.requiredFiniteArray("max", componentCount)
    minimum.indices.forEach { index ->
        require(minimum[index] <= maximum[index]) {
            "The model result has invalid accessor bounds"
        }
        if (componentType == GLB_FLOAT) {
            require(
                kotlin.math.abs(minimum[index]) <= CREATION_GLB_MAXIMUM_ABSOLUTE_RENDERER_VALUE &&
                    kotlin.math.abs(maximum[index]) <= CREATION_GLB_MAXIMUM_ABSOLUTE_RENDERER_VALUE
            ) { "The model result has excessive accessor bounds" }
        }
    }
    return minimum to maximum
}

private fun validateGlbUnsupportedWork(document: JSONObject) {
    listOf("animations", "skins", "cameras").forEach { name ->
        val values = document.optionalArray(name)
        require(values.length() == 0) {
            "The model result contains unsupported runtime work"
        }
    }
}

internal fun decodeGlbDataUri(uri: String, prefixes: Set<String>): ByteArray {
    require(uri.length <= CREATION_GLB_MAXIMUM_DATA_URI_CHARACTERS) {
        "The model result contains too much embedded data"
    }
    val prefix = prefixes.firstOrNull { uri.startsWith(it, ignoreCase = true) }
    requireNotNull(prefix) { "The model result contains an external resource" }
    return runCatching { Base64.getDecoder().decode(uri.substring(prefix.length)) }
        .getOrNull()
        ?.takeIf(ByteArray::isNotEmpty)
        ?: error("The model result contains invalid embedded data")
}

private fun glbElementBytes(type: String, componentBytes: Int): Int {
    if (type == "MAT2" || type == "MAT3") {
        val rows = if (type == "MAT2") 2 else 3
        val columnBytes = Math.multiplyExact(rows, componentBytes)
        val alignedColumnBytes = Math.multiplyExact((columnBytes + 3) / 4, 4)
        return Math.multiplyExact(alignedColumnBytes, rows)
    }
    return Math.multiplyExact(requireNotNull(GLB_TYPE_COMPONENTS[type]), componentBytes)
}

internal fun JSONObject.requiredArray(name: String, minimum: Int, maximum: Int): JSONArray =
    requireNotNull(optJSONArray(name)).also {
        require(it.length() in minimum..maximum) { "The model result metadata is too complex" }
    }

internal fun JSONObject.optionalArray(name: String): JSONArray =
    if (has(name)) {
        requireNotNull(optJSONArray(name)) { "The model result metadata is invalid" }
    } else {
        JSONArray()
    }

internal fun JSONObject.requiredIndex(name: String, size: Int): Int =
    requiredLong(name, 0, (size - 1).toLong()).toInt()

internal fun JSONArray.requiredIndex(index: Int, size: Int): Int =
    requiredLong(index, 0, (size - 1).toLong()).toInt()

internal fun JSONObject.requiredLong(name: String, minimum: Long, maximum: Long): Long =
    jsonInteger(opt(name)).also { require(it in minimum..maximum) }

internal fun JSONObject.optionalLong(name: String, minimum: Long, maximum: Long): Long =
    if (has(name)) requiredLong(name, minimum, maximum) else minimum

internal fun JSONObject.requiredFiniteArray(name: String, length: Int): DoubleArray {
    val values = requireNotNull(optJSONArray(name)) {
        "The model result contains invalid numeric metadata"
    }
    require(values.length() == length) {
        "The model result contains invalid numeric metadata"
    }
    return DoubleArray(length) { index ->
        val value = values.opt(index)
        require(value is Number) { "The model result contains invalid numeric metadata" }
        value.toDouble().also {
            require(it.isFinite()) { "The model result contains invalid numeric metadata" }
        }
    }
}

internal fun JSONArray.requiredLong(index: Int, minimum: Long, maximum: Long): Long =
    jsonInteger(opt(index)).also { require(it in minimum..maximum) }

private fun jsonInteger(value: Any?): Long {
    require(value is Number) { "The model result contains invalid numeric metadata" }
    return value.toString().toLongOrNull()
        ?: error("The model result contains invalid numeric metadata")
}

internal fun JSONArray.requiredObject(index: Int): JSONObject =
    requireNotNull(optJSONObject(index)) { "The model result metadata is invalid" }

internal fun checkedAdd(left: Long, right: Long): Long =
    runCatching { Math.addExact(left, right) }
        .getOrElse { error("The model result metadata is too large") }

internal fun checkedMultiply(left: Long, right: Long): Long =
    runCatching { Math.multiplyExact(left, right) }
        .getOrElse { error("The model result metadata is too large") }

internal data class GlbBuffer(
    val length: Long,
    val binary: CreationGlbBinaryChunk?,
    val embedded: ByteArray?,
)

internal data class GlbBufferView(
    val buffer: Int,
    val offset: Long,
    val length: Long,
    val stride: Int,
)

internal data class GlbAccessor(
    val type: String,
    val componentType: Int,
    val componentCount: Int,
    val count: Long,
    val view: Int,
    val offset: Long,
    val absoluteOffset: Long,
    val elementBytes: Int,
    val stride: Int,
    val normalized: Boolean,
    val minimum: DoubleArray?,
    val maximum: DoubleArray?,
)

internal data class GlbMeshCost(
    val vertices: Long,
    val indices: Long,
    val morphElements: Long,
    val morphTargets: Int,
)

private val GLB_COMPONENT_BYTES = mapOf(
    5_120 to 1,
    5_121 to 1,
    5_122 to 2,
    5_123 to 2,
    5_125 to 4,
    5_126 to 4,
)
private val GLB_TYPE_COMPONENTS = mapOf(
    "SCALAR" to 1,
    "VEC2" to 2,
    "VEC3" to 3,
    "VEC4" to 4,
    "MAT2" to 4,
    "MAT3" to 9,
    "MAT4" to 16,
)
private val GLB_BUFFER_TARGETS = setOf(34_962, 34_963)
internal val GLB_INDEX_COMPONENTS = setOf(5_121, 5_123, 5_125)
internal val GLB_MORPH_ATTRIBUTES = setOf("POSITION", "NORMAL", "TANGENT")
internal const val GLB_FLOAT = 5_126
internal const val GLB_MODE_LINES = 1
internal const val GLB_MODE_TRIANGLES = 4
internal const val CREATION_GLB_MAXIMUM_JSON_BYTES = 8 * 1024 * 1024
internal const val CREATION_GLB_MAXIMUM_DATA_URI_CHARACTERS = 2_800_000
private const val MAXIMUM_GLB_JSON_DEPTH = 128
internal const val CREATION_GLB_MAXIMUM_BUFFERS = 64
internal const val CREATION_GLB_MAXIMUM_BUFFER_VIEWS = 32_768
internal const val CREATION_GLB_MAXIMUM_AGGREGATE_BUFFER_VIEW_BYTES = 104_857_600L
internal const val CREATION_GLB_MAXIMUM_ACCESSORS = 16_384
internal const val CREATION_GLB_MAXIMUM_ACCESSOR_ELEMENTS = 12_000_000L
internal const val CREATION_GLB_MAXIMUM_ABSOLUTE_RENDERER_VALUE = 10_000_000.0
private const val MAXIMUM_GLB_BUFFERS = CREATION_GLB_MAXIMUM_BUFFERS
private const val MAXIMUM_GLB_BUFFER_VIEWS = CREATION_GLB_MAXIMUM_BUFFER_VIEWS
private const val MAXIMUM_GLB_ACCESSORS = CREATION_GLB_MAXIMUM_ACCESSORS
private const val MAXIMUM_GLB_ACCESSOR_COUNT = CREATION_GLB_MAXIMUM_ACCESSOR_ELEMENTS
private const val MAXIMUM_GLB_ACCESSOR_ELEMENTS = CREATION_GLB_MAXIMUM_ACCESSOR_ELEMENTS
private const val MAXIMUM_GLB_BYTE_STRIDE = 252
internal const val CREATION_GLB_MAXIMUM_MESHES = 1_024
internal const val CREATION_GLB_MAXIMUM_PRIMITIVES = 4_096
internal const val CREATION_GLB_MAXIMUM_MATERIALS = 1_024
internal const val CREATION_GLB_MAXIMUM_VERTICES = 2_000_000L
internal const val CREATION_GLB_MAXIMUM_INDICES = 6_000_000L
internal const val CREATION_GLB_MAXIMUM_MORPH_TARGETS = 256
internal const val CREATION_GLB_MAXIMUM_MORPH_ELEMENTS = 8_000_000L
internal const val CREATION_GLB_MAXIMUM_NODES = 4_096
internal const val CREATION_GLB_MAXIMUM_SCENES = 64
internal const val CREATION_GLB_MAXIMUM_PRIMITIVE_ATTRIBUTES = 16
internal const val CREATION_GLB_MAXIMUM_MORPH_ATTRIBUTES = 8
internal const val CREATION_GLB_MAXIMUM_NODE_DEPTH = 256

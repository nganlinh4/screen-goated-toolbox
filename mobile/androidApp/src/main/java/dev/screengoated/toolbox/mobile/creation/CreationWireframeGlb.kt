package dev.screengoated.toolbox.mobile.creation

import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject

internal object CreationWireframeGlb {
    fun create(source: File, target: File): File {
        require(source.isFile && source.length() in MINIMUM_GLB_BYTES..MAXIMUM_GLB_BYTES) {
            "Model preview is unavailable"
        }
        val bytes = source.readBytes()
        val parsed = parse(bytes)
        val document = parsed.document
        val binary = ByteArrayOutputStream(parsed.binary.size + 256 * 1024).apply {
            write(parsed.binary)
        }
        val accessors = document.optJSONArray("accessors") ?: JSONArray().also {
            document.put("accessors", it)
        }
        val bufferViews = document.optJSONArray("bufferViews") ?: JSONArray().also {
            document.put("bufferViews", it)
        }
        val meshes = document.getJSONArray("meshes")
        var converted = 0
        for (meshIndex in 0 until meshes.length()) {
            val primitives = meshes.getJSONObject(meshIndex).getJSONArray("primitives")
            for (primitiveIndex in 0 until primitives.length()) {
                val primitive = primitives.getJSONObject(primitiveIndex)
                if (primitive.optInt("mode", TRIANGLES) != TRIANGLES) continue
                val positionAccessor = primitive.getJSONObject("attributes").getInt("POSITION")
                val positionCount = accessors.getJSONObject(positionAccessor).getInt("count")
                val triangles = triangleIndices(
                    primitive,
                    positionCount,
                    accessors,
                    bufferViews,
                    parsed.binary,
                )
                val edges = uniqueEdges(triangles)
                if (edges.isEmpty()) continue
                align(binary)
                val offset = binary.size()
                val maximumIndex = edges.maxOf { maxOf(it.first, it.second) }
                val componentType = if (maximumIndex <= U16_MAXIMUM) UNSIGNED_SHORT else UNSIGNED_INT
                writeEdges(binary, edges, componentType)
                val byteLength = binary.size() - offset
                val viewIndex = bufferViews.length()
                bufferViews.put(
                    JSONObject()
                        .put("buffer", 0)
                        .put("byteOffset", offset)
                        .put("byteLength", byteLength)
                        .put("target", ELEMENT_ARRAY_BUFFER),
                )
                val accessorIndex = accessors.length()
                accessors.put(
                    JSONObject()
                        .put("bufferView", viewIndex)
                        .put("byteOffset", 0)
                        .put("componentType", componentType)
                        .put("count", edges.size * 2)
                        .put("type", "SCALAR"),
                )
                primitive.put("indices", accessorIndex)
                primitive.put("mode", LINES)
                converted += 1
            }
        }
        require(converted > 0) { "Model has no triangle geometry for wireframe preview" }
        val binaryBytes = binary.toByteArray()
        document.getJSONArray("buffers").getJSONObject(0).put("byteLength", binaryBytes.size)
        val output = encode(document, binaryBytes)
        require(output.size.toLong() <= MAXIMUM_GLB_BYTES) { "Wireframe preview is too large" }
        writeAtomic(target, output)
        return target
    }

    private fun triangleIndices(
        primitive: JSONObject,
        positionCount: Int,
        accessors: JSONArray,
        bufferViews: JSONArray,
        binary: ByteArray,
    ): IntArray {
        val accessorIndex = primitive.optInt("indices", -1)
        if (accessorIndex < 0) {
            require(positionCount % 3 == 0) { "Non-indexed triangle geometry is incomplete" }
            return IntArray(positionCount) { it }
        }
        val accessor = accessors.getJSONObject(accessorIndex)
        require(!accessor.has("sparse")) { "Sparse wireframe indices are unsupported" }
        val count = accessor.getInt("count")
        require(count > 0 && count % 3 == 0) { "Triangle index count is invalid" }
        val componentType = accessor.getInt("componentType")
        val componentBytes = when (componentType) {
            UNSIGNED_BYTE -> 1
            UNSIGNED_SHORT -> 2
            UNSIGNED_INT -> 4
            else -> error("Triangle indices use an unsupported component type")
        }
        val view = bufferViews.getJSONObject(accessor.getInt("bufferView"))
        require(view.optInt("buffer", 0) == 0) { "Wireframe source is not embedded in the GLB" }
        val stride = view.optInt("byteStride", componentBytes)
        require(stride >= componentBytes) { "Triangle index stride is invalid" }
        val start = view.optInt("byteOffset", 0) + accessor.optInt("byteOffset", 0)
        require(start >= 0 && start + (count - 1) * stride + componentBytes <= binary.size) {
            "Triangle indices exceed the GLB buffer"
        }
        val source = ByteBuffer.wrap(binary).order(ByteOrder.LITTLE_ENDIAN)
        return IntArray(count) { index ->
            val offset = start + index * stride
            val value = when (componentType) {
                UNSIGNED_BYTE -> binary[offset].toInt() and 0xff
                UNSIGNED_SHORT -> source.getShort(offset).toInt() and 0xffff
                else -> {
                    val unsigned = source.getInt(offset).toLong() and 0xffff_ffffL
                    require(unsigned <= Int.MAX_VALUE) { "Wireframe index is too large" }
                    unsigned.toInt()
                }
            }
            require(value < positionCount) { "Triangle index exceeds its position accessor" }
            value
        }
    }

    private fun uniqueEdges(indices: IntArray): List<Pair<Int, Int>> {
        val keys = LinkedHashSet<Long>(indices.size)
        indices.asList().chunked(3).forEach { triangle ->
            addEdge(keys, triangle[0], triangle[1])
            addEdge(keys, triangle[1], triangle[2])
            addEdge(keys, triangle[2], triangle[0])
        }
        return keys.map { key -> (key ushr 32).toInt() to key.toInt() }
    }

    private fun addEdge(keys: MutableSet<Long>, left: Int, right: Int) {
        if (left == right) return
        val minimum = minOf(left, right)
        val maximum = maxOf(left, right)
        keys += (minimum.toLong() shl 32) or (maximum.toLong() and 0xffff_ffffL)
    }

    private fun writeEdges(
        output: ByteArrayOutputStream,
        edges: List<Pair<Int, Int>>,
        componentType: Int,
    ) {
        val componentBytes = if (componentType == UNSIGNED_SHORT) 2 else 4
        val buffer = ByteBuffer.allocate(edges.size * 2 * componentBytes)
            .order(ByteOrder.LITTLE_ENDIAN)
        edges.forEach { (left, right) ->
            if (componentType == UNSIGNED_SHORT) {
                buffer.putShort(left.toShort())
                buffer.putShort(right.toShort())
            } else {
                buffer.putInt(left)
                buffer.putInt(right)
            }
        }
        output.write(buffer.array())
    }

    private fun parse(bytes: ByteArray): ParsedGlb {
        require(bytes.size >= MINIMUM_GLB_BYTES) { "Model is not a GLB" }
        val header = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        require(header.int == GLB_MAGIC && header.int == GLB_VERSION && header.int == bytes.size) {
            "Model GLB header is invalid"
        }
        var offset = 12
        var document: JSONObject? = null
        var binary: ByteArray? = null
        while (offset + 8 <= bytes.size) {
            val length = header.getInt(offset)
            val type = header.getInt(offset + 4)
            offset += 8
            require(length >= 0 && offset + length <= bytes.size) { "Model GLB chunk is invalid" }
            val chunk = bytes.copyOfRange(offset, offset + length)
            when (type) {
                JSON_CHUNK -> document = JSONObject(
                    chunk.toString(Charsets.UTF_8).trimEnd('\u0000', ' ', '\t', '\r', '\n'),
                )
                BIN_CHUNK -> binary = chunk
            }
            offset += length
        }
        val parsedDocument = requireNotNull(document) { "Model GLB has no JSON document" }
        val parsedBinary = requireNotNull(binary) { "Model GLB has no embedded buffer" }
        require(
            parsedDocument.getJSONArray("buffers").getJSONObject(0).optString("uri").isEmpty(),
        ) { "Model GLB uses an external buffer" }
        return ParsedGlb(parsedDocument, parsedBinary)
    }

    private fun encode(document: JSONObject, rawBinary: ByteArray): ByteArray {
        val json = document.toString().toByteArray(Charsets.UTF_8).padToFour(' '.code.toByte())
        val binary = rawBinary.padToFour(0)
        val total = 12 + 8 + json.size + 8 + binary.size
        return ByteBuffer.allocate(total).order(ByteOrder.LITTLE_ENDIAN).apply {
            putInt(GLB_MAGIC)
            putInt(GLB_VERSION)
            putInt(total)
            putInt(json.size)
            putInt(JSON_CHUNK)
            put(json)
            putInt(binary.size)
            putInt(BIN_CHUNK)
            put(binary)
        }.array()
    }

    private fun ByteArray.padToFour(value: Byte): ByteArray =
        copyOf(size + ((4 - size % 4) % 4)).also { padded ->
            for (index in size until padded.size) padded[index] = value
        }

    private fun align(output: ByteArrayOutputStream) {
        repeat((4 - output.size() % 4) % 4) { output.write(0) }
    }

    private fun writeAtomic(target: File, bytes: ByteArray) {
        target.parentFile?.mkdirs()
        val temporary = File(target.parentFile, "${target.name}.tmp-${UUID.randomUUID()}")
        try {
            FileOutputStream(temporary).use { output ->
                output.write(bytes)
                output.fd.sync()
            }
            if (!temporary.renameTo(target)) {
                temporary.copyTo(target, overwrite = true)
                temporary.delete()
            }
        } finally {
            temporary.delete()
        }
    }

    private data class ParsedGlb(val document: JSONObject, val binary: ByteArray)

    private const val MINIMUM_GLB_BYTES = 20L
    private const val MAXIMUM_GLB_BYTES = 200L * 1024 * 1024
    private const val GLB_MAGIC = 0x46546c67
    private const val GLB_VERSION = 2
    private const val JSON_CHUNK = 0x4e4f534a
    private const val BIN_CHUNK = 0x004e4942
    private const val TRIANGLES = 4
    private const val LINES = 1
    private const val ELEMENT_ARRAY_BUFFER = 34_963
    private const val UNSIGNED_BYTE = 5_121
    private const val UNSIGNED_SHORT = 5_123
    private const val UNSIGNED_INT = 5_125
    private const val U16_MAXIMUM = 65_535
}

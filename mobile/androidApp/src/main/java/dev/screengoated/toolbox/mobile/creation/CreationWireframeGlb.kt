package dev.screengoated.toolbox.mobile.creation

import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileOutputStream
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject

internal object CreationWireframeGlb {
    fun create(source: File, target: File): File {
        CreationArtifactValidator.validateGlb(source)
        require(source.length() in MINIMUM_GLB_BYTES..MAXIMUM_WIREFRAME_SOURCE_BYTES) {
            "Model preview is unavailable"
        }
        val parsed = parse(source)
        val document = parsed.document
        val appended = ByteArrayOutputStream()
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
                align(appended)
                val offset = Math.addExact(parsed.binary.size, appended.size())
                val maximumIndex = edges.maxOf { maxOf(it.first, it.second) }
                val componentType = if (maximumIndex <= U16_MAXIMUM) UNSIGNED_SHORT else UNSIGNED_INT
                val before = appended.size()
                writeEdges(appended, edges, componentType)
                require(appended.size() <= MAXIMUM_WIREFRAME_ADDED_BYTES) {
                    "Model is too complex for wireframe preview"
                }
                val byteLength = appended.size() - before
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
                        .put("count", Math.multiplyExact(edges.size, 2))
                        .put("type", "SCALAR"),
                )
                primitive.put("indices", accessorIndex)
                primitive.put("mode", LINES)
                converted += 1
            }
        }
        require(converted > 0) { "Model has no triangle geometry for wireframe preview" }
        val appendedBytes = appended.toByteArray()
        document.getJSONArray("buffers").getJSONObject(0)
            .put("byteLength", Math.addExact(parsed.binary.size, appendedBytes.size))
        writeAtomic(target, document, parsed.binary, appendedBytes)
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
            require(
                positionCount > 0 &&
                    positionCount % 3 == 0 &&
                    positionCount <= MAXIMUM_TRIANGLE_INDICES,
            ) { "Non-indexed triangle geometry is too complex" }
            return IntArray(positionCount) { it }
        }
        val accessor = accessors.getJSONObject(accessorIndex)
        require(!accessor.has("sparse")) { "Sparse wireframe indices are unsupported" }
        val count = accessor.getInt("count")
        require(count > 0 && count % 3 == 0 && count <= MAXIMUM_TRIANGLE_INDICES) {
            "Model is too complex for wireframe preview"
        }
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
        val start = checkedWireframeAdd(
            view.optLong("byteOffset", 0L),
            accessor.optLong("byteOffset", 0L),
        )
        val end = checkedWireframeAdd(
            start,
            checkedWireframeAdd(
                checkedWireframeMultiply(count - 1L, stride.toLong()),
                componentBytes.toLong(),
            ),
        )
        require(start >= 0L && end <= binary.size.toLong()) {
            "Triangle indices exceed the GLB buffer"
        }
        val source = ByteBuffer.wrap(binary).order(ByteOrder.LITTLE_ENDIAN)
        return IntArray(count) { index ->
            val offset = checkedWireframeAdd(
                start,
                checkedWireframeMultiply(index.toLong(), stride.toLong()),
            ).toInt()
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
        val key = (minimum.toLong() shl 32) or (maximum.toLong() and 0xffff_ffffL)
        require(key in keys || keys.size < MAXIMUM_UNIQUE_EDGES) {
            "Model is too complex for wireframe preview"
        }
        keys += key
    }

    private fun writeEdges(
        output: ByteArrayOutputStream,
        edges: List<Pair<Int, Int>>,
        componentType: Int,
    ) {
        val componentBytes = if (componentType == UNSIGNED_SHORT) 2 else 4
        val buffer = ByteBuffer.allocate(
            Math.multiplyExact(Math.multiplyExact(edges.size, 2), componentBytes),
        )
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

    private fun parse(file: File): ParsedGlb = RandomAccessFile(file, "r").use { input ->
        val length = input.length()
        val header = ByteArray(12).also(input::readFully).littleEndian()
        require(
            header.int == GLB_MAGIC &&
                header.int == GLB_VERSION &&
                (header.int.toLong() and 0xffff_ffffL) == length,
        ) { "Model GLB header is invalid" }
        val jsonLength = readChunkHeader(
            input,
            JSON_CHUNK,
            length,
            MAXIMUM_JSON_BYTES.toLong(),
        )
        val document = JSONObject(
            ByteArray(jsonLength).also(input::readFully)
                .toString(Charsets.UTF_8)
                .trimEnd('\u0000', ' ', '\t', '\r', '\n'),
        )
        val binaryLength = readChunkHeader(
            input,
            BIN_CHUNK,
            length,
            MAXIMUM_WIREFRAME_SOURCE_BYTES,
        )
        val binary = ByteArray(binaryLength).also(input::readFully)
        require(input.filePointer == length) { "Model GLB has unsupported chunks" }
        ParsedGlb(document, binary)
    }

    private fun ByteArray.padToFour(value: Byte): ByteArray =
        copyOf(size + ((4 - size % 4) % 4)).also { padded ->
            for (index in size until padded.size) padded[index] = value
        }

    private fun align(output: ByteArrayOutputStream) {
        repeat((4 - output.size() % 4) % 4) { output.write(0) }
    }

    private fun readChunkHeader(
        input: RandomAccessFile,
        expectedType: Int,
        fileLength: Long,
        maximumLength: Long,
    ): Int {
        require(fileLength - input.filePointer >= 8) { "Model GLB chunk is missing" }
        val chunk = ByteArray(8).also(input::readFully).littleEndian()
        val length = chunk.int.toLong() and 0xffff_ffffL
        require(
            chunk.int == expectedType &&
                length % 4L == 0L &&
                length <= maximumLength &&
                length <= fileLength - input.filePointer,
        ) { "Model GLB chunk is invalid" }
        return length.toInt()
    }

    private fun writeAtomic(
        target: File,
        document: JSONObject,
        originalBinary: ByteArray,
        appendedBinary: ByteArray,
    ) {
        val json = document.toString().toByteArray(Charsets.UTF_8).padToFour(' '.code.toByte())
        require(json.size <= MAXIMUM_JSON_BYTES) { "Wireframe preview metadata is too large" }
        val binaryLength = Math.addExact(originalBinary.size, appendedBinary.size)
        val binaryPadding = (4 - binaryLength % 4) % 4
        val paddedBinaryLength = Math.addExact(binaryLength, binaryPadding)
        val total = listOf(12, 8, json.size, 8, paddedBinaryLength)
            .fold(0, Math::addExact)
        require(total.toLong() <= MAXIMUM_GLB_BYTES) { "Wireframe preview is too large" }
        target.parentFile?.mkdirs()
        val temporary = File(target.parentFile, "${target.name}.tmp-${UUID.randomUUID()}")
        try {
            FileOutputStream(temporary).use { output ->
                output.write(littleEndian(GLB_MAGIC, GLB_VERSION, total, json.size, JSON_CHUNK))
                output.write(json)
                output.write(littleEndian(paddedBinaryLength, BIN_CHUNK))
                output.write(originalBinary)
                output.write(appendedBinary)
                repeat(binaryPadding) { output.write(0) }
                output.fd.sync()
            }
            runCatching {
                Files.move(
                    temporary.toPath(),
                    target.toPath(),
                    StandardCopyOption.ATOMIC_MOVE,
                    StandardCopyOption.REPLACE_EXISTING,
                )
            }.getOrElse {
                Files.move(
                    temporary.toPath(),
                    target.toPath(),
                    StandardCopyOption.REPLACE_EXISTING,
                )
            }
        } finally {
            temporary.delete()
        }
    }

    private data class ParsedGlb(val document: JSONObject, val binary: ByteArray)

    private fun checkedWireframeAdd(left: Long, right: Long): Long =
        runCatching { Math.addExact(left, right) }
            .getOrElse { error("Wireframe preview metadata is too large") }

    private fun checkedWireframeMultiply(left: Long, right: Long): Long =
        runCatching { Math.multiplyExact(left, right) }
            .getOrElse { error("Wireframe preview metadata is too large") }

    private fun ByteArray.littleEndian(): ByteBuffer =
        ByteBuffer.wrap(this).order(ByteOrder.LITTLE_ENDIAN)

    private fun littleEndian(vararg values: Int): ByteArray =
        ByteBuffer.allocate(values.size * Int.SIZE_BYTES).order(ByteOrder.LITTLE_ENDIAN)
            .apply { values.forEach(::putInt) }
            .array()

    private const val MINIMUM_GLB_BYTES = 20L
    private const val MAXIMUM_GLB_BYTES = CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES
    private const val MAXIMUM_WIREFRAME_SOURCE_BYTES = 32L * 1024 * 1024
    private const val MAXIMUM_JSON_BYTES = 8 * 1024 * 1024
    private const val MAXIMUM_TRIANGLE_INDICES = 300_000
    private const val MAXIMUM_UNIQUE_EDGES = 300_000
    private const val MAXIMUM_WIREFRAME_ADDED_BYTES = 8 * 1024 * 1024
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

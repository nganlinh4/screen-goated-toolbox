package dev.screengoated.toolbox.mobile.creation

import android.graphics.BitmapFactory
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.json.JSONArray
import org.json.JSONObject

internal fun validateGlbImagesAndTextures(
    document: JSONObject,
    input: RandomAccessFile,
    buffers: List<GlbBuffer>,
    views: List<GlbBufferView>,
) {
    val images = document.optionalGlbImageArray("images")
    require(images.length() <= MAXIMUM_GLB_IMAGES) { "The model result has too many images" }
    var encodedBytes = 0L
    var decodedPixels = 0L
    val imagePixels = mutableListOf<Long>()
    for (index in 0 until images.length()) {
        val image = images.requiredGlbImageObject(index)
        val hasUri = image.has("uri")
        val uri = if (hasUri) {
            requireNotNull(image.opt("uri") as? String) {
                "The model result has an invalid image"
            }
        } else {
            ""
        }
        val viewIndex = if (image.has("bufferView")) {
            image.requiredGlbImageIndex("bufferView", views.size)
        } else {
            -1
        }
        require(hasUri xor (viewIndex >= 0)) { "The model result has an invalid image" }
        val declaredMime = if (image.has("mimeType")) {
            (image.opt("mimeType") as? String)
                ?.takeIf(String::isNotBlank)
                ?: error("The model result has an invalid image")
        } else {
            null
        }
        val uriMime = if (hasUri) {
            GLB_IMAGE_DATA_PREFIX_MIME.entries
                .firstOrNull { uri.startsWith(it.key, ignoreCase = true) }
                ?.value
                ?: error("The model result contains an external resource")
        } else {
            null
        }
        val bytes = if (hasUri) {
            decodeGlbDataUri(uri, GLB_IMAGE_DATA_PREFIX_MIME.keys)
        } else {
            require(declaredMime in GLB_IMAGE_MIME_TYPES) { "The model result has an invalid image" }
            readGlbImageView(input, buffers, views[viewIndex])
        }
        require(bytes.size in 1..MAXIMUM_GLB_IMAGE_BYTES) { "The model result image is too large" }
        val dimensions = detectGlbImage(bytes)
        require(dimensions.mime != "image/png" || !containsGlbPngChunk(bytes, "acTL")) {
            "The model result contains an animated texture"
        }
        require(dimensions.mime != "image/webp" || !containsGlbWebpAnimation(bytes)) {
            "The model result contains an animated texture"
        }
        require(declaredMime == null || declaredMime == dimensions.mime) {
            "The model result has conflicting image metadata"
        }
        require(uriMime == null || uriMime == dimensions.mime) {
            "The model result has conflicting image metadata"
        }
        val pixels = glbImageCheckedMultiply(dimensions.width.toLong(), dimensions.height.toLong())
        require(
            dimensions.width in 1..MAXIMUM_GLB_IMAGE_DIMENSION &&
                dimensions.height in 1..MAXIMUM_GLB_IMAGE_DIMENSION &&
                pixels <= MAXIMUM_GLB_IMAGE_PIXELS
        ) { "The model result image dimensions are too large" }
        encodedBytes = glbImageCheckedAdd(encodedBytes, bytes.size.toLong())
        decodedPixels = glbImageCheckedAdd(decodedPixels, pixels)
        require(
            encodedBytes <= MAXIMUM_GLB_TOTAL_IMAGE_BYTES &&
                decodedPixels <= MAXIMUM_GLB_DECODED_IMAGE_PIXELS
        ) { "The model result contains too much image data" }
        validateGlbImageDecode(bytes, dimensions)
        imagePixels += pixels
    }
    val samplers = document.optionalGlbImageArray("samplers")
    require(samplers.length() <= MAXIMUM_GLB_SAMPLERS) {
        "The model result has too many texture samplers"
    }
    repeat(samplers.length()) { validateGlbSampler(samplers.requiredGlbImageObject(it)) }
    val textures = document.optionalGlbImageArray("textures")
    require(textures.length() <= MAXIMUM_GLB_TEXTURES) { "The model result has too many textures" }
    var texturePixels = 0L
    val texturePixelsByIndex = mutableListOf<Long>()
    for (index in 0 until textures.length()) {
        val texture = textures.requiredGlbImageObject(index)
        val sources = buildList {
            if (texture.has("source")) {
                add(texture.requiredGlbImageIndex("source", images.length()))
            }
            texture.optJSONObject("extensions")
                ?.optJSONObject("EXT_texture_webp")
                ?.let { extension ->
                    add(extension.requiredGlbImageIndex("source", images.length()))
                }
        }
        require(sources.isNotEmpty()) { "The model result has an invalid texture" }
        val pixels = sources.maxOf(imagePixels::get)
        texturePixels = glbImageCheckedAdd(texturePixels, pixels)
        require(texturePixels <= MAXIMUM_GLB_REFERENCED_TEXTURE_PIXELS) {
            "The model result contains too much texture data"
        }
        texturePixelsByIndex += pixels
        if (texture.has("sampler")) texture.requiredGlbImageIndex("sampler", samplers.length())
    }
    validateGlbMaterialTextures(document, texturePixelsByIndex, texturePixels)
}

internal fun validateGlbSampler(sampler: JSONObject) {
    if (sampler.has("name")) {
        require(sampler.opt("name") is String) {
            "The model result has invalid texture sampler metadata"
        }
    }
    mapOf(
        "magFilter" to setOf(9728, 9729),
        "minFilter" to setOf(9728, 9729, 9984, 9985, 9986, 9987),
        "wrapS" to setOf(33071, 33648, 10497),
        "wrapT" to setOf(33071, 33648, 10497),
    ).forEach { (field, allowed) ->
        if (sampler.has(field)) {
            val value = sampler.opt(field)
            require(value is Number && value.toString().toIntOrNull() in allowed) {
                "The model result has invalid texture sampler metadata"
            }
        }
    }
}

private fun validateGlbImageDecode(bytes: ByteArray, dimensions: GlbImageDimensions) {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    require(bounds.outWidth == dimensions.width && bounds.outHeight == dimensions.height) {
        "The model result contains an invalid texture"
    }
    val bitmap = BitmapFactory.decodeByteArray(
        bytes,
        0,
        bytes.size,
        BitmapFactory.Options(),
    )
    requireNotNull(bitmap) { "The model result contains an invalid texture" }.recycle()
}

private fun readGlbImageView(
    input: RandomAccessFile,
    buffers: List<GlbBuffer>,
    view: GlbBufferView,
): ByteArray {
    require(view.length <= MAXIMUM_GLB_IMAGE_BYTES) { "The model result image is too large" }
    val buffer = buffers[view.buffer]
    buffer.embedded?.let {
        val start = view.offset.toInt()
        return it.copyOfRange(start, Math.addExact(start, view.length.toInt()))
    }
    val binary = requireNotNull(buffer.binary)
    val start = glbImageCheckedAdd(binary.offset, view.offset)
    require(glbImageCheckedAdd(start, view.length) <= glbImageCheckedAdd(binary.offset, binary.length)) {
        "The model result image exceeds its buffer"
    }
    return ByteArray(view.length.toInt()).also {
        input.seek(start)
        input.readFully(it)
    }
}

private fun detectGlbImage(bytes: ByteArray): GlbImageDimensions = when {
    bytes.size >= 24 && bytes.copyOfRange(0, 8).contentEquals(GLB_PNG_SIGNATURE) -> {
        val buffer = ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN)
        require(buffer.getInt(12) == 0x49484452) { "The model result has an invalid PNG texture" }
        GlbImageDimensions("image/png", buffer.getInt(16), buffer.getInt(20))
    }
    bytes.size >= 12 && bytes[0] == 0xff.toByte() && bytes[1] == 0xd8.toByte() ->
        detectGlbJpeg(bytes)
    bytes.size >= 16 &&
        bytes.copyOfRange(0, 4).decodeToString() == "RIFF" &&
        bytes.copyOfRange(8, 12).decodeToString() == "WEBP" -> detectGlbWebp(bytes)
    else -> error("The model result has an invalid image")
}

private fun containsGlbPngChunk(bytes: ByteArray, requestedType: String): Boolean {
    var position = GLB_PNG_SIGNATURE.size
    while (position <= bytes.size - 12) {
        val length = ByteBuffer.wrap(bytes, position, 4)
            .order(ByteOrder.BIG_ENDIAN)
            .int
            .toLong() and 0xffff_ffffL
        if (length > bytes.size - position - 12L) return false
        if (bytes.decodeToString(position + 4, position + 8) == requestedType) return true
        position = Math.addExact(position, Math.addExact(12, length.toInt()))
    }
    return false
}

private fun containsGlbWebpAnimation(bytes: ByteArray): Boolean {
    if (
        bytes.size < 12 ||
        bytes.copyOfRange(0, 4).decodeToString() != "RIFF" ||
        bytes.copyOfRange(8, 12).decodeToString() != "WEBP"
    ) {
        return false
    }
    var position = 12
    while (position <= bytes.size - 8) {
        val type = bytes.decodeToString(position, position + 4)
        val length = ByteBuffer.wrap(bytes, position + 4, 4)
            .order(ByteOrder.LITTLE_ENDIAN)
            .int
            .toLong() and 0xffff_ffffL
        if (length > bytes.size - position - 8L) return true
        if (type == "ANIM" || type == "ANMF") return true
        if (
            type == "VP8X" &&
            length > 0 &&
            bytes[position + 8].toInt() and 0x02 != 0
        ) {
            return true
        }
        position = Math.addExact(
            position,
            Math.addExact(8, length.toInt()) + (length % 2L).toInt(),
        )
    }
    return false
}

private fun detectGlbJpeg(bytes: ByteArray): GlbImageDimensions {
    var offset = 2
    while (offset + 4 <= bytes.size) {
        while (offset < bytes.size && bytes[offset] != 0xff.toByte()) offset += 1
        while (offset < bytes.size && bytes[offset] == 0xff.toByte()) offset += 1
        if (offset >= bytes.size) break
        val marker = bytes[offset].toInt() and 0xff
        offset += 1
        if (marker in GLB_JPEG_STANDALONE_MARKERS) continue
        require(offset + 2 <= bytes.size) { "The model result has an invalid JPEG texture" }
        val length = unsignedShort(bytes, offset)
        require(length >= 2 && offset + length <= bytes.size) {
            "The model result has an invalid JPEG texture"
        }
        if (marker in GLB_JPEG_SIZE_MARKERS) {
            require(length >= 7) { "The model result has an invalid JPEG texture" }
            return GlbImageDimensions(
                "image/jpeg",
                unsignedShort(bytes, offset + 5),
                unsignedShort(bytes, offset + 3),
            )
        }
        offset += length
    }
    error("The model result has an invalid JPEG texture")
}

private fun detectGlbWebp(bytes: ByteArray): GlbImageDimensions {
    val tag = bytes.copyOfRange(12, 16).decodeToString()
    return when (tag) {
        "VP8X" -> {
            require(bytes.size >= 30)
            GlbImageDimensions(
                "image/webp",
                1 + unsigned24(bytes, 24),
                1 + unsigned24(bytes, 27),
            )
        }
        "VP8L" -> {
            require(bytes.size >= 25 && bytes[20] == 0x2f.toByte())
            val packed = ByteBuffer.wrap(bytes, 21, 4).order(ByteOrder.LITTLE_ENDIAN).int
            GlbImageDimensions(
                "image/webp",
                1 + (packed and 0x3fff),
                1 + ((packed ushr 14) and 0x3fff),
            )
        }
        "VP8 " -> {
            require(
                bytes.size >= 30 &&
                    bytes[23] == 0x9d.toByte() &&
                    bytes[24] == 0x01.toByte() &&
                    bytes[25] == 0x2a.toByte()
            )
            GlbImageDimensions(
                "image/webp",
                unsignedShortLittleEndian(bytes, 26) and 0x3fff,
                unsignedShortLittleEndian(bytes, 28) and 0x3fff,
            )
        }
        else -> error("The model result has an invalid WebP texture")
    }
}

private fun JSONObject.requiredGlbImageIndex(name: String, size: Int): Int {
    require(size > 0)
    val value = opt(name)
    require(value is Number)
    return value.toString().toIntOrNull()?.also { require(it in 0 until size) }
        ?: error("The model result has invalid image metadata")
}

private fun JSONArray.requiredGlbImageObject(index: Int): JSONObject =
    requireNotNull(optJSONObject(index)) { "The model result metadata is invalid" }

private fun JSONObject.optionalGlbImageArray(name: String): JSONArray =
    if (has(name)) {
        requireNotNull(optJSONArray(name)) { "The model result metadata is invalid" }
    } else {
        JSONArray()
    }

private fun glbImageCheckedAdd(left: Long, right: Long): Long =
    runCatching { Math.addExact(left, right) }
        .getOrElse { error("The model result image metadata is too large") }

private fun glbImageCheckedMultiply(left: Long, right: Long): Long =
    runCatching { Math.multiplyExact(left, right) }
        .getOrElse { error("The model result image metadata is too large") }

private fun unsignedShort(bytes: ByteArray, offset: Int): Int =
    ((bytes[offset].toInt() and 0xff) shl 8) or (bytes[offset + 1].toInt() and 0xff)

private fun unsignedShortLittleEndian(bytes: ByteArray, offset: Int): Int =
    (bytes[offset].toInt() and 0xff) or ((bytes[offset + 1].toInt() and 0xff) shl 8)

private fun unsigned24(bytes: ByteArray, offset: Int): Int =
    (bytes[offset].toInt() and 0xff) or
        ((bytes[offset + 1].toInt() and 0xff) shl 8) or
        ((bytes[offset + 2].toInt() and 0xff) shl 16)

private data class GlbImageDimensions(val mime: String, val width: Int, val height: Int)

private val GLB_IMAGE_MIME_TYPES = setOf("image/png", "image/jpeg", "image/webp")
private val GLB_IMAGE_DATA_PREFIX_MIME = mapOf(
    "data:image/png;base64," to "image/png",
    "data:image/jpeg;base64," to "image/jpeg",
    "data:image/webp;base64," to "image/webp",
)
private val GLB_PNG_SIGNATURE = byteArrayOf(
    0x89.toByte(), 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
)
private val GLB_JPEG_STANDALONE_MARKERS = setOf(
    0x01, 0xd8, 0xd9, 0xd0, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7,
)
private val GLB_JPEG_SIZE_MARKERS = setOf(
    0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf,
)
internal const val CREATION_GLB_MAXIMUM_IMAGES = 256
internal const val CREATION_GLB_MAXIMUM_TEXTURES = 512
internal const val CREATION_GLB_MAXIMUM_SAMPLERS = 128
private const val MAXIMUM_GLB_IMAGES = CREATION_GLB_MAXIMUM_IMAGES
private const val MAXIMUM_GLB_TEXTURES = CREATION_GLB_MAXIMUM_TEXTURES
private const val MAXIMUM_GLB_SAMPLERS = CREATION_GLB_MAXIMUM_SAMPLERS
private const val MAXIMUM_GLB_IMAGE_BYTES = 16 * 1024 * 1024
private const val MAXIMUM_GLB_TOTAL_IMAGE_BYTES = 64L * 1024 * 1024
internal const val CREATION_GLB_MAXIMUM_IMAGE_DIMENSION = 8_192
internal const val CREATION_GLB_MAXIMUM_IMAGE_PIXELS = 16_777_216L
internal const val CREATION_GLB_MAXIMUM_DECODED_IMAGE_PIXELS = 33_554_432L
internal const val CREATION_GLB_MAXIMUM_REFERENCED_TEXTURE_PIXELS = 33_554_432L
private const val MAXIMUM_GLB_IMAGE_DIMENSION = CREATION_GLB_MAXIMUM_IMAGE_DIMENSION
private const val MAXIMUM_GLB_IMAGE_PIXELS = CREATION_GLB_MAXIMUM_IMAGE_PIXELS
private const val MAXIMUM_GLB_DECODED_IMAGE_PIXELS =
    CREATION_GLB_MAXIMUM_DECODED_IMAGE_PIXELS
private const val MAXIMUM_GLB_REFERENCED_TEXTURE_PIXELS =
    CREATION_GLB_MAXIMUM_REFERENCED_TEXTURE_PIXELS

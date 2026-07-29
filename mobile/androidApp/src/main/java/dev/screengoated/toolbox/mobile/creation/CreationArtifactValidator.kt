package dev.screengoated.toolbox.mobile.creation

import android.graphics.BitmapFactory
import android.util.Base64
import java.io.ByteArrayInputStream
import java.io.File
import java.io.InputStream
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.charset.CodingErrorAction
import java.nio.charset.StandardCharsets
import java.util.IdentityHashMap
import java.util.zip.CRC32
import javax.xml.parsers.DocumentBuilderFactory
import org.json.JSONObject
import org.json.JSONTokener
import org.w3c.dom.Element
import org.w3c.dom.Node

internal data class CreationImageDimensions(val width: Int, val height: Int)

internal object CreationArtifactValidator {
    private const val GLB_MAGIC = 0x46546C67
    private const val GLB_JSON_CHUNK = 0x4E4F534A
    private const val GLB_BINARY_CHUNK = 0x004E4942
    private const val PNG_IHDR = "IHDR"
    private const val PNG_IDAT = "IDAT"
    private const val PNG_IEND = "IEND"
    private val pngSignature = byteArrayOf(
        0x89.toByte(),
        0x50,
        0x4E,
        0x47,
        0x0D,
        0x0A,
        0x1A,
        0x0A,
    )
    private val allowedGlbDataUriPrefixes = setOf(
        "data:application/octet-stream;base64,",
        "data:image/png;base64,",
        "data:image/jpeg;base64,",
        "data:image/webp;base64,",
    )

    fun validateGlb(file: File) {
        require(
            file.isFile &&
                file.length() in 20..CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES,
        ) { "The model result is incomplete" }
        RandomAccessFile(file, "r").use { input ->
            val fileLength = input.length()
            require(fileLength in 20..CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES) {
                "The model result is incomplete"
            }
            val header = ByteArray(12).also(input::readFully).littleEndian()
            require(header.int == GLB_MAGIC && header.int == 2) { "The model result is invalid" }
            val declaredLength = header.int.toLong() and 0xffffffffL
            require(declaredLength == fileLength) { "The model result is incomplete" }
            val jsonHeader = readGlbChunkHeader(input, declaredLength)
            require(
                jsonHeader.type == GLB_JSON_CHUNK &&
                    jsonHeader.length <= CREATION_GLB_MAXIMUM_JSON_BYTES,
            ) { "The model result is invalid" }
            val jsonBytes = ByteArray(jsonHeader.length.toInt()).also(input::readFully)
            validateCreationGlbJsonEnvelope(jsonBytes)
            val document = decodeCreationGlbDocument(jsonBytes)
            val binaryChunk = if (input.filePointer < declaredLength) {
                val binaryHeader = readGlbChunkHeader(input, declaredLength)
                require(binaryHeader.type == GLB_BINARY_CHUNK) { "The model result is invalid" }
                val chunk = CreationGlbBinaryChunk(input.filePointer, binaryHeader.length)
                input.seek(Math.addExact(chunk.offset, chunk.length))
                chunk
            } else {
                null
            }
            require(input.filePointer == declaredLength) {
                "The model result contains unsupported chunks"
            }
            val asset = requireNotNull(document.optJSONObject("asset")) {
                "The model result uses an unsupported format"
            }
            require(asset.opt("version") == "2.0") {
                "The model result uses an unsupported format"
            }
            require(!asset.has("minVersion") || asset.opt("minVersion") == "2.0") {
                "The model result uses an unsupported format"
            }
            val meshes = document.optJSONArray("meshes")
            require(meshes != null && meshes.length() > 0) { "The model result has no geometry" }
            var hasPositions = false
            for (meshIndex in 0 until meshes.length()) {
                val primitives = meshes.optJSONObject(meshIndex)?.optJSONArray("primitives") ?: continue
                for (primitiveIndex in 0 until primitives.length()) {
                    val attributes = primitives.optJSONObject(primitiveIndex)
                        ?.optJSONObject("attributes")
                    if (attributes?.has("POSITION") == true) hasPositions = true
                }
            }
            require(hasPositions) { "The model result has no renderable geometry" }
            validateCreationGlbDocument(document, input, binaryChunk)
            validateNoExternalGlbUris(document)
        }
    }

    private fun decodeCreationGlbDocument(bytes: ByteArray): JSONObject {
        var end = bytes.size
        while (end > 0 && bytes[end - 1] == 0x20.toByte()) end -= 1
        require(end > 0) { "The model result is invalid" }
        val text = StandardCharsets.UTF_8
            .newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT)
            .decode(ByteBuffer.wrap(bytes, 0, end))
            .toString()
        require(text.lastOrNull() == '}') { "The model result is invalid" }
        val parser = JSONTokener(text)
        val value = parser.nextValue()
        require(value is JSONObject && parser.nextClean() == '\u0000') {
            "The model result is invalid"
        }
        return value
    }

    private fun readGlbChunkHeader(
        input: RandomAccessFile,
        declaredLength: Long,
    ): GlbChunkHeader {
        require(declaredLength - input.filePointer >= 8L) { "The model result is invalid" }
        val chunkHeader = ByteArray(8).also(input::readFully).littleEndian()
        val length = chunkHeader.int.toLong() and 0xffffffffL
        val type = chunkHeader.int
        require(
            length % 4L == 0L &&
                length <= declaredLength - input.filePointer,
        ) { "The model result is incomplete" }
        return GlbChunkHeader(length, type)
    }

    fun validateSvg(file: File) {
        require(file.isFile && file.length() in 1..CreationContract.MAXIMUM_SVG_ARTIFACT_BYTES) {
            "The vector result is incomplete"
        }
        file.inputStream().use(::validateSvgStream)
    }

    fun validateSvgText(svg: String) {
        val bytes = svg.encodeToByteArray()
        require(bytes.size.toLong() in 1..CreationContract.MAXIMUM_SVG_ARTIFACT_BYTES) {
            "The vector result is incomplete"
        }
        ByteArrayInputStream(bytes).use(::validateSvgStream)
    }

    private fun validateSvgStream(input: InputStream) {
        val bytes = input.readBytes()
        require(bytes.size.toLong() in 1..CreationContract.MAXIMUM_SVG_ARTIFACT_BYTES) {
            "The vector result is incomplete"
        }
        validateCreationSvgSecurityPreflight(bytes)
        val document = safeSvgDocumentBuilderFactory()
            .newDocumentBuilder()
            .parse(ByteArrayInputStream(bytes))
        val root = document.documentElement
        require(
            root.localName.equals("svg", ignoreCase = true) &&
                root.namespaceURI == SVG_NAMESPACE &&
                root.getAttribute("xmlns") == SVG_NAMESPACE,
        ) { "The vector result is invalid" }
        validateCreationSvgReferenceGraph(root)
        val state = SvgValidationState()
        validateSvgNode(root, state)
        validateCreationSvgReferenceExpansion(root, state.rasterPixelsByElement)
    }

    fun validatePng(file: File, eventWidth: Int?, eventHeight: Int?): CreationImageDimensions {
        require(
            file.isFile &&
                file.length() in 32..CreationContract.MAXIMUM_IMAGE_ARTIFACT_BYTES,
        ) { "The image result is incomplete" }
        validatePngStructure(file)
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.absolutePath, bounds)
        validateImageDimensions(bounds.outWidth, bounds.outHeight, "image result")
        require(
            (eventWidth == null && eventHeight == null) ||
                (eventWidth == bounds.outWidth && eventHeight == bounds.outHeight),
        ) {
            "The image result has conflicting dimensions"
        }
        val sample = boundedDecodeSample(bounds.outWidth, bounds.outHeight)
        val decoded = BitmapFactory.decodeFile(
            file.absolutePath,
            BitmapFactory.Options().apply { inSampleSize = sample },
        )
        requireNotNull(decoded) { "The image result is invalid" }.recycle()
        return CreationImageDimensions(bounds.outWidth, bounds.outHeight)
    }

    private fun validatePngStructure(file: File) {
        RandomAccessFile(file, "r").use { input ->
            val signature = ByteArray(pngSignature.size).also(input::readFully)
            require(signature.contentEquals(pngSignature)) { "The image result is not a PNG image" }
            var chunkIndex = 0
            var hasImageData = false
            var reachedEnd = false
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            while (!reachedEnd) {
                require(file.length() - input.filePointer >= 12L) {
                    "The image result is incomplete"
                }
                val length = input.readInt().toLong() and 0xffffffffL
                val typeBytes = ByteArray(4).also(input::readFully)
                val type = typeBytes.decodeToString()
                require(length <= file.length() - input.filePointer - 4L) {
                    "The image result is incomplete"
                }
                require(chunkIndex != 0 || type == PNG_IHDR && length == 13L) {
                    "The image result is invalid"
                }
                val crc = CRC32().apply { update(typeBytes) }
                var remaining = length
                while (remaining > 0L) {
                    val count = minOf(remaining, buffer.size.toLong()).toInt()
                    input.readFully(buffer, 0, count)
                    crc.update(buffer, 0, count)
                    remaining -= count
                }
                val expectedCrc = input.readInt().toLong() and 0xffffffffL
                require(crc.value == expectedCrc) { "The image result is corrupt" }
                hasImageData = hasImageData || type == PNG_IDAT
                reachedEnd = type == PNG_IEND
                if (reachedEnd) {
                    require(length == 0L && input.filePointer == file.length()) {
                        "The image result is invalid"
                    }
                }
                chunkIndex += 1
            }
            require(hasImageData) { "The image result is incomplete" }
        }
    }

    private fun validateNoExternalGlbUris(value: Any?) {
        when (value) {
            is JSONObject -> value.keys().forEach { key ->
                val child = value.opt(key)
                if (key.equals("uri", ignoreCase = true) && child is String && child.isNotBlank()) {
                    validateGlbDataUri(child, allowedGlbDataUriPrefixes)
                } else {
                    validateNoExternalGlbUris(child)
                }
            }
            is org.json.JSONArray -> {
                for (index in 0 until value.length()) validateNoExternalGlbUris(value.opt(index))
            }
        }
    }

    private fun validateGlbDataUri(uri: String, allowedPrefixes: Set<String>) {
        require(uri.length <= CREATION_GLB_MAXIMUM_DATA_URI_CHARACTERS) {
            "The model result contains too much embedded data"
        }
        val prefix = allowedPrefixes.firstOrNull { uri.startsWith(it, ignoreCase = true) }
        requireNotNull(prefix) { "The model result contains an external resource" }
        val decoded = runCatching {
            java.util.Base64.getDecoder().decode(uri.substring(prefix.length))
        }.getOrNull()
        require(decoded != null && decoded.isNotEmpty()) {
            "The model result contains invalid embedded data"
        }
    }

    private fun validateImageDimensions(width: Int, height: Int, subject: String) {
        require(width > 0 && height > 0) { "The $subject is invalid" }
        require(
            width <= CreationContract.MAXIMUM_IMAGE_DIMENSION &&
                height <= CreationContract.MAXIMUM_IMAGE_DIMENSION,
        ) { "The $subject dimensions are too large" }
        require(width.toLong() * height <= CreationContract.MAXIMUM_DECODED_IMAGE_PIXELS) {
            "The $subject is too large"
        }
    }

    private fun boundedDecodeSample(width: Int, height: Int): Int {
        var sample = 1
        while (width / sample > MAXIMUM_VALIDATION_DECODE_EDGE ||
            height / sample > MAXIMUM_VALIDATION_DECODE_EDGE
        ) {
            sample *= 2
        }
        return sample
    }

    private fun validateSvgNode(node: Node, state: SvgValidationState) {
        require(node.nodeType != Node.PROCESSING_INSTRUCTION_NODE) {
            "The vector result contains active content"
        }
        if (node.nodeType == Node.ELEMENT_NODE) {
            val element = node as Element
            val localName = element.localName.lowercase()
            require(
                element.namespaceURI == SVG_NAMESPACE &&
                localName !in activeSvgElements &&
                    localName != "filter" &&
                    !localName.startsWith("fe") &&
                    element.namespaceURI != XINCLUDE_NAMESPACE,
            ) {
                "The vector result contains active content"
            }
            for (index in 0 until element.attributes.length) {
                val attribute = element.attributes.item(index)
                val rawName = attribute.nodeName
                val name = rawName.lowercase()
                val value = attribute.nodeValue.trim()
                if (rawName.equals("xmlns", ignoreCase = true) ||
                    rawName.startsWith("xmlns:", ignoreCase = true)
                ) {
                    require(
                        rawName == "xmlns" && value == SVG_NAMESPACE ||
                            rawName == "xmlns:xlink" && value == XLINK_NAMESPACE,
                    ) { "The vector result contains an unsupported namespace" }
                    continue
                }
                if (rawName.contains(':')) {
                    require(
                        rawName in setOf("xml:lang", "xml:space") ||
                            rawName == "xlink:href" &&
                            attribute.namespaceURI == XLINK_NAMESPACE,
                    ) { "The vector result contains an unsupported namespace" }
                }
                require(!name.startsWith("on") && name != "filter") {
                    "The vector result contains active content"
                }
                require(name !in forbiddenResourceAttributes || value.isBlank()) {
                    "The vector result contains an external resource"
                }
                val isHref = name == "href" || name.endsWith(":href")
                val embeddedPixels = if (localName == "image" && isHref) {
                    validateInlineImage(value, state)
                } else {
                    null
                }
                val safeHrefImage = embeddedPixels != null
                if (embeddedPixels != null) {
                    state.rasterPixelsByElement[element] =
                        (state.rasterPixelsByElement[element] ?: 0L) + embeddedPixels
                }
                if (isHref) {
                    require(
                        value.isBlank() ||
                            value.startsWith("#") ||
                            safeHrefImage,
                    ) {
                        "The vector result contains an external resource"
                    }
                }
                require(safeHrefImage || !containsUnsafeUrl(value)) {
                    "The vector result contains an external resource"
                }
                if (name == "style") {
                    require(!containsCssMotion(value) && !containsCssFilter(value)) {
                        "The vector result contains unsupported motion"
                    }
                }
            }
            if (localName == "style") {
                val stylesheet = element.textContent.orEmpty()
                require(
                    !CSS_URL_START.containsMatchIn(stylesheet) &&
                        !containsUnsafeUrl(stylesheet) &&
                        !containsCssMotion(stylesheet) &&
                        !containsCssFilter(stylesheet),
                ) {
                    "The vector result contains unsupported stylesheet resources"
                }
            }
        }
        var child = node.firstChild
        while (child != null) {
            validateSvgNode(child, state)
            child = child.nextSibling
        }
    }

    private fun containsUnsafeUrl(value: String): Boolean {
        if ('\\' in value) return true
        if (CSS_COMMENT.containsMatchIn(value)) return true
        val normalized = value.trim()
        val lower = normalized.lowercase()
        if ("javascript:" in lower || "@import" in lower || "expression(" in lower ||
            "behavior:" in lower || "-moz-binding" in lower || "image-set(" in lower
        ) return true
        val calls = CSS_URL.findAll(normalized).toList()
        if (CSS_URL_START.containsMatchIn(normalized) && calls.isEmpty()) return true
        if (RAW_RESOURCE_SCHEME.containsMatchIn(normalized) && calls.isEmpty()) return true
        return calls.any { match ->
            val target = match.groupValues[1].trim().trim('\'', '"')
            target.isNotBlank() && !target.startsWith("#")
        }
    }

    private fun containsCssMotion(value: String): Boolean =
        CSS_KEYFRAMES.containsMatchIn(value) ||
            CSS_MOTION_PROPERTY.containsMatchIn(value)

    private fun containsCssFilter(value: String): Boolean =
        value.lowercase().filterNot(Char::isWhitespace).contains("filter:")

    private fun validateInlineImage(value: String, state: SvgValidationState): Long? {
        val marker = inlineImagePrefixes.firstOrNull { value.startsWith(it, ignoreCase = true) }
            ?: return null
        require(value.length <= CreationContract.MAXIMUM_SVG_EMBEDDED_RASTER_CHARACTERS) {
            "The vector result contains too much embedded image data"
        }
        val pixels = state.validatedInlineImages[value] ?: run {
            val bytes = runCatching {
                Base64.decode(value.substring(marker.length), Base64.DEFAULT)
            }.getOrNull() ?: return null
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
            val expectedMime = if (marker.contains("png", ignoreCase = true)) {
                "image/png"
            } else {
                "image/jpeg"
            }
            val pixels = bounds.outWidth.toLong() * bounds.outHeight
            require(
                bounds.outWidth > 0 &&
                    bounds.outHeight > 0 &&
                    bounds.outMimeType == expectedMime &&
                    pixels <= CreationContract.MAXIMUM_SVG_EMBEDDED_RASTER_PIXELS,
            ) { "The vector result contains an invalid embedded image" }
            require(expectedMime != "image/png" || !containsPngChunk(bytes, "acTL")) {
                "The vector result contains animated embedded content"
            }
            val sample = boundedDecodeSample(bounds.outWidth, bounds.outHeight)
            requireNotNull(
                BitmapFactory.decodeByteArray(
                    bytes,
                    0,
                    bytes.size,
                    BitmapFactory.Options().apply { inSampleSize = sample },
                ),
            ) { "The vector result contains an invalid embedded image" }.recycle()
            state.validatedInlineImages[value] = pixels
            pixels
        }
        state.totalEmbeddedRasterPixels = chargeCreationSvgEmbeddedRasterOccurrence(
            state.totalEmbeddedRasterPixels,
            pixels,
        )
        return pixels
    }

    private fun containsPngChunk(bytes: ByteArray, requestedType: String): Boolean {
        if (bytes.size < pngSignature.size || !bytes.copyOf(pngSignature.size)
                .contentEquals(pngSignature)
        ) return false
        var position = pngSignature.size
        while (position <= bytes.size - 12) {
            val length = ByteBuffer.wrap(bytes, position, 4).int.toLong() and 0xffffffffL
            if (length > bytes.size - position - 12L) return false
            val type = bytes.decodeToString(position + 4, position + 8)
            if (type == requestedType) return true
            position += (12L + length).toInt()
        }
        return false
    }

    private fun ByteArray.littleEndian(): ByteBuffer =
        ByteBuffer.wrap(this).order(ByteOrder.LITTLE_ENDIAN)

    private val activeSvgElements = setOf(
        "script",
        "foreignobject",
        "iframe",
        "object",
        "embed",
        "canvas",
        "feimage",
        "audio",
        "video",
        "animate",
        "animatemotion",
        "animatetransform",
        "set",
        "discard",
    )

    private const val MAXIMUM_VALIDATION_DECODE_EDGE = 2_048
    private const val SVG_NAMESPACE = "http://www.w3.org/2000/svg"
    private const val XLINK_NAMESPACE = "http://www.w3.org/1999/xlink"
    private const val XINCLUDE_NAMESPACE = "http://www.w3.org/2001/XInclude"
    private val forbiddenResourceAttributes = setOf(
        "src",
        "data",
        "poster",
        "formaction",
        "xml:base",
    )
    private val inlineImagePrefixes = listOf(
        "data:image/png;base64,",
        "data:image/jpeg;base64,",
    )
    private val CSS_COMMENT = Regex("""/\*[\s\S]*?\*/""")
    private val CSS_URL_START = Regex("""url\s*\(""", RegexOption.IGNORE_CASE)
    private val CSS_URL = Regex("""url\s*\(\s*([^)]*?)\s*\)""", RegexOption.IGNORE_CASE)
    private val CSS_KEYFRAMES = Regex(
        """@(?:-[a-z]+-)?keyframes\b""",
        RegexOption.IGNORE_CASE,
    )
    private val CSS_MOTION_PROPERTY = Regex(
        """(?:^|[;{])\s*(?:-[a-z]+-)?(?:animation|transition)(?:-[a-z-]+)?\s*:""",
        RegexOption.IGNORE_CASE,
    )
    private val RAW_RESOURCE_SCHEME = Regex(
        """(?:https?|file|content|ftp|blob|javascript|data):""",
        RegexOption.IGNORE_CASE,
    )

    private data class SvgValidationState(
        var totalEmbeddedRasterPixels: Long = 0,
        val validatedInlineImages: MutableMap<String, Long> = mutableMapOf(),
        val rasterPixelsByElement: IdentityHashMap<Element, Long> = IdentityHashMap(),
    )

    private data class GlbChunkHeader(val length: Long, val type: Int)
}

internal fun chargeCreationSvgEmbeddedRasterOccurrence(total: Long, pixels: Long): Long {
    require(
        pixels in 1..CreationContract.MAXIMUM_SVG_EMBEDDED_RASTER_PIXELS &&
            pixels <= CreationContract.MAXIMUM_SVG_TOTAL_EMBEDDED_RASTER_PIXELS - total,
    ) { "The vector result contains too much embedded image data" }
    return total + pixels
}

internal fun safeSvgDocumentBuilderFactory(): DocumentBuilderFactory =
    DocumentBuilderFactory.newInstance().apply {
        isNamespaceAware = true
        isExpandEntityReferences = false
        runCatching { isXIncludeAware = false }
        setFeature("http://apache.org/xml/features/disallow-doctype-decl", true)
        setFeature("http://xml.org/sax/features/external-general-entities", false)
        setFeature("http://xml.org/sax/features/external-parameter-entities", false)
        setFeature("http://apache.org/xml/features/nonvalidating/load-external-dtd", false)
        runCatching {
            setAttribute("http://javax.xml.XMLConstants/property/accessExternalDTD", "")
        }
        runCatching {
            setAttribute("http://javax.xml.XMLConstants/property/accessExternalSchema", "")
        }
    }

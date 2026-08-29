package dev.screengoated.toolbox.mobile.creation

import android.graphics.Matrix
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Region
import android.util.Xml
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.core.graphics.PathParser
import java.io.StringReader
import java.io.StringWriter
import java.util.Locale
import javax.xml.transform.OutputKeys
import javax.xml.transform.TransformerFactory
import javax.xml.transform.dom.DOMSource
import javax.xml.transform.stream.StreamResult
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.hypot
import kotlin.math.min
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.xmlpull.v1.XmlPullParser

@Composable
internal fun CreationSvgDocument(
    outputPath: String,
    viewModel: CreationNativeViewModel,
    controller: CreationSvgDocumentController,
    editingRequested: Boolean,
    modifier: Modifier = Modifier,
) {
    val svg by produceState<String?>(null, outputPath) {
        value = runCatching {
            viewModel.readSvg(outputPath).also { text ->
                withContext(Dispatchers.Default) {
                    CreationArtifactValidator.validateSvgText(text)
                }
            }
        }.getOrNull()
    }
    val document by produceState<NativeSvgDocument?>(null, outputPath, editingRequested, svg) {
        val source = svg
        value = if (editingRequested && source != null) {
            runCatching {
                withContext(Dispatchers.Default) {
                    NativeSvgParser.parse(source)
                }
            }.getOrNull()
        } else {
            null
        }
    }
    val source = svg
    if (source == null) {
        Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            androidx.compose.material3.CircularProgressIndicator()
        }
        return
    }
    LaunchedEffect(document) {
        document?.let(controller::attach)
    }
    val revision = controller.revision
    CreationSvgFullFidelitySurface(
        svg = source,
        document = document,
        controller = controller,
        revision = revision,
        modifier = modifier.fillMaxSize(),
    )
}

internal data class NativeSvgDocument(
    val originalSvg: String,
    val viewBox: RectF,
    val width: String?,
    val height: String?,
    val shapes: MutableList<NativeSvgShape>,
    val editable: Boolean,
) {
    fun viewportTransform(width: Float, height: Float, zoom: Float, pan: Offset): SvgViewportTransform {
        val base = min(width / viewBox.width().coerceAtLeast(1f), height / viewBox.height().coerceAtLeast(1f)) * 0.94f
        val contentWidth = viewBox.width() * base
        val contentHeight = viewBox.height() * base
        return SvgViewportTransform(
            scale = base * zoom,
            origin = Offset((width - contentWidth * zoom) / 2f, (height - contentHeight * zoom) / 2f),
            pan = pan,
            documentOrigin = Offset(viewBox.left, viewBox.top),
        )
    }

    fun hitTest(point: Offset, tolerance: Float): Int? = shapes.indices.reversed().firstOrNull { index ->
        val shape = shapes[index]
        !shape.deleted && shape.contains(point, tolerance)
    }

    fun editAt(index: Int): SvgShapeEdit? = shapes.getOrNull(index)?.edit()

    fun applyEdit(index: Int, edit: SvgShapeEdit) {
        shapes.getOrNull(index)?.apply(edit)
    }

    fun serialize(): String = serializeEditedSvg(originalSvg, shapes)

    fun serializationSnapshot(): NativeSvgDocument = copy(
        shapes = shapes.map { it.copy() }.toMutableList(),
    )
}

internal data class NativeSvgShape(
    val documentIndex: Int,
    val tag: String,
    val geometry: Map<String, String>,
    val matrix: Matrix,
    val path: Path,
    var fill: String,
    var stroke: String,
    val strokeWidth: Float,
    val opacity: Float,
    var deleted: Boolean = false,
    val originalEdit: SvgShapeEdit = SvgShapeEdit(fill, stroke, deleted),
) {
    fun contains(point: Offset, tolerance: Float): Boolean {
        val bounds = RectF()
        path.computeBounds(bounds, true)
        if (!bounds.apply { inset(-tolerance, -tolerance) }.contains(point.x, point.y)) return false
        if (fill.equals("none", true)) return true
        val clip = Region(
            floor(bounds.left).toInt(),
            floor(bounds.top).toInt(),
            ceil(bounds.right).toInt(),
            ceil(bounds.bottom).toInt(),
        )
        return Region().apply { setPath(path, clip) }.contains(point.x.toInt(), point.y.toInt())
    }

    fun edit() = SvgShapeEdit(fill, stroke, deleted)

    fun apply(edit: SvgShapeEdit) {
        fill = edit.fill
        stroke = edit.stroke
        deleted = edit.deleted
    }
}

internal data class SvgViewportTransform(
    val scale: Float,
    val origin: Offset,
    val pan: Offset,
    val documentOrigin: Offset,
) {
    fun toDocument(point: Offset) = Offset(
        (point.x - origin.x - pan.x) / scale + documentOrigin.x,
        (point.y - origin.y - pan.y) / scale + documentOrigin.y,
    )
    fun documentTolerance(screenPixels: Float) = screenPixels / scale.coerceAtLeast(0.001f)
}

internal data class SvgShapeEdit(val fill: String, val stroke: String, val deleted: Boolean)

internal object NativeSvgParser {
    internal val shapeTags = setOf(
        "path",
        "rect",
        "circle",
        "ellipse",
        "line",
        "polyline",
        "polygon",
    )
    private val transformPattern = Regex("([a-zA-Z]+)\\s*\\(([^)]*)\\)")
    private val numberPattern = Regex("[-+]?(?:\\d*\\.)?\\d+(?:[eE][-+]?\\d+)?")

    fun parse(svg: String): NativeSvgDocument {
        val parser = Xml.newPullParser().apply {
            setFeature(XmlPullParser.FEATURE_PROCESS_NAMESPACES, false)
            setInput(StringReader(svg))
        }
        var viewBox = RectF(0f, 0f, 1f, 1f)
        var width: String? = null
        var height: String? = null
        val matrices = ArrayDeque<Matrix>()
        val styles = ArrayDeque<SvgStyle>()
        val shapes = mutableListOf<NativeSvgShape>()
        val withinByteBudget =
            svg.encodeToByteArray().size.toLong() <= CreationContract.MAXIMUM_EDITABLE_SVG_BYTES
        var geometryElements = 0
        var event = parser.eventType
        while (event != XmlPullParser.END_DOCUMENT) {
            when (event) {
                XmlPullParser.START_TAG -> {
                    val tag = parser.name.substringAfter(':').lowercase(Locale.ROOT)
                    if (tag == "svg") {
                        width = parser.attribute("width")
                        height = parser.attribute("height")
                        viewBox = parseViewBox(parser.attribute("viewBox"), width, height)
                    }
                    val matrix = Matrix(matrices.lastOrNull() ?: Matrix()).apply {
                        parser.attribute("transform")?.let { postConcat(parseTransform(it)) }
                    }
                    val style = (styles.lastOrNull() ?: SvgStyle()).merged(parser)
                    matrices.addLast(matrix)
                    styles.addLast(style)
                    if (tag in shapeTags) {
                        geometryElements += 1
                        if (withinByteBudget &&
                            geometryElements <= CreationContract.MAXIMUM_EDITABLE_SVG_GEOMETRY
                        ) {
                            createShape(
                                geometryElements - 1,
                                tag,
                                parser,
                                matrix,
                                style,
                            )?.let(shapes::add)
                        }
                    }
                }
                XmlPullParser.END_TAG -> {
                    matrices.removeLastOrNull()
                    styles.removeLastOrNull()
                }
            }
            event = parser.next()
        }
        val editable = withinByteBudget &&
            geometryElements <= CreationContract.MAXIMUM_EDITABLE_SVG_GEOMETRY
        if (!editable) shapes.clear()
        return NativeSvgDocument(svg, viewBox, width, height, shapes, editable)
    }

    private fun createShape(
        documentIndex: Int,
        tag: String,
        parser: XmlPullParser,
        matrix: Matrix,
        style: SvgStyle,
    ): NativeSvgShape? {
        val geometryNames = when (tag) {
            "path" -> listOf("d")
            "rect" -> listOf("x", "y", "width", "height", "rx", "ry")
            "circle" -> listOf("cx", "cy", "r")
            "ellipse" -> listOf("cx", "cy", "rx", "ry")
            "line" -> listOf("x1", "y1", "x2", "y2")
            else -> listOf("points")
        }
        val geometry = geometryNames.mapNotNull { name -> parser.attribute(name)?.let { name to it } }.toMap()
        val path = when (tag) {
            "path" -> geometry["d"]?.let(PathParser::createPathFromPathData)
            "rect" -> Path().apply {
                val x = geometry.number("x")
                val y = geometry.number("y")
                val w = geometry.number("width")
                val h = geometry.number("height")
                val rx = geometry.number("rx")
                val ry = geometry["ry"]?.toFloatOrNull() ?: rx
                addRoundRect(RectF(x, y, x + w, y + h), rx, ry, Path.Direction.CW)
            }
            "circle" -> Path().apply { addCircle(geometry.number("cx"), geometry.number("cy"), geometry.number("r"), Path.Direction.CW) }
            "ellipse" -> Path().apply {
                val cx = geometry.number("cx")
                val cy = geometry.number("cy")
                val rx = geometry.number("rx")
                val ry = geometry.number("ry")
                addOval(RectF(cx - rx, cy - ry, cx + rx, cy + ry), Path.Direction.CW)
            }
            "line" -> Path().apply { moveTo(geometry.number("x1"), geometry.number("y1")); lineTo(geometry.number("x2"), geometry.number("y2")) }
            "polyline", "polygon" -> pointsPath(geometry["points"].orEmpty(), tag == "polygon")
            else -> null
        } ?: return null
        path.transform(matrix)
        val values = FloatArray(9).also(matrix::getValues)
        val lineScale = ((hypot(values[0].toDouble(), values[3].toDouble()) + hypot(values[1].toDouble(), values[4].toDouble())) / 2.0).toFloat()
        return NativeSvgShape(
            documentIndex = documentIndex,
            tag = tag,
            geometry = geometry,
            matrix = Matrix(matrix),
            path = path,
            fill = style.fill,
            stroke = style.stroke,
            strokeWidth = style.strokeWidth * lineScale.coerceAtLeast(0.01f),
            opacity = style.opacity,
        )
    }

    private fun pointsPath(value: String, close: Boolean): Path? {
        val numbers = numberPattern.findAll(value).map { it.value.toFloat() }.toList()
        if (numbers.size < 4) return null
        return Path().apply {
            moveTo(numbers[0], numbers[1])
            var index = 2
            while (index + 1 < numbers.size) { lineTo(numbers[index], numbers[index + 1]); index += 2 }
            if (close) close()
        }
    }

    private fun parseViewBox(value: String?, width: String?, height: String?): RectF {
        val values = value?.let { numberPattern.findAll(it).map { match -> match.value.toFloat() }.toList() }.orEmpty()
        if (values.size >= 4 && values[2] > 0f && values[3] > 0f) {
            return RectF(values[0], values[1], values[0] + values[2], values[1] + values[3])
        }
        val w = width?.let(::svgLength) ?: 1024f
        val h = height?.let(::svgLength) ?: 1024f
        return RectF(0f, 0f, w.coerceAtLeast(1f), h.coerceAtLeast(1f))
    }

    private fun parseTransform(value: String): Matrix {
        val result = Matrix()
        transformPattern.findAll(value).forEach { match ->
            val name = match.groupValues[1].lowercase(Locale.ROOT)
            val values = numberPattern.findAll(match.groupValues[2]).map { it.value.toFloat() }.toList()
            val next = Matrix()
            when (name) {
                "matrix" -> if (values.size >= 6) next.setValues(floatArrayOf(values[0], values[2], values[4], values[1], values[3], values[5], 0f, 0f, 1f))
                "translate" -> next.setTranslate(values.getOrElse(0) { 0f }, values.getOrElse(1) { 0f })
                "scale" -> next.setScale(values.getOrElse(0) { 1f }, values.getOrElse(1) { values.getOrElse(0) { 1f } })
                "rotate" -> if (values.size >= 3) next.setRotate(values[0], values[1], values[2]) else next.setRotate(values.getOrElse(0) { 0f })
                "skewx" -> next.setSkew(kotlin.math.tan(Math.toRadians(values.getOrElse(0) { 0f }.toDouble())).toFloat(), 0f)
                "skewy" -> next.setSkew(0f, kotlin.math.tan(Math.toRadians(values.getOrElse(0) { 0f }.toDouble())).toFloat())
            }
            result.postConcat(next)
        }
        return result
    }
}

private data class SvgStyle(
    val fill: String = "#000000",
    val stroke: String = "none",
    val strokeWidth: Float = 1f,
    val opacity: Float = 1f,
) {
    fun merged(parser: XmlPullParser): SvgStyle {
        val declarations = parser.attribute("style")
            ?.split(';')
            ?.mapNotNull { item -> item.split(':', limit = 2).takeIf { it.size == 2 }?.let { it[0].trim() to it[1].trim() } }
            ?.toMap()
            .orEmpty()
        fun value(name: String) = parser.attribute(name) ?: declarations[name]
        return SvgStyle(
            fill = value("fill") ?: fill,
            stroke = value("stroke") ?: stroke,
            strokeWidth = value("stroke-width")?.let(::svgLength) ?: strokeWidth,
            opacity = (opacity * (value("opacity")?.toFloatOrNull() ?: 1f)).coerceIn(0f, 1f),
        )
    }
}

private fun XmlPullParser.attribute(name: String): String? =
    (0 until attributeCount).firstOrNull { getAttributeName(it).substringAfter(':').equals(name, true) }
        ?.let { getAttributeValue(it) }

private fun Map<String, String>.number(name: String): Float = get(name)?.let(::svgLength) ?: 0f

private fun svgLength(value: String): Float = Regex("[-+]?(?:\\d*\\.)?\\d+(?:[eE][-+]?\\d+)?")
    .find(value)?.value?.toFloatOrNull() ?: 0f

private fun serializeEditedSvg(svg: String, shapes: List<NativeSvgShape>): String {
    if (shapes.none { it.edit() != it.originalEdit }) return svg
    val document = safeSvgDocumentBuilderFactory()
        .newDocumentBuilder()
        .parse(svg.byteInputStream())
    val shapesByDocumentIndex = shapes.associateBy(NativeSvgShape::documentIndex)
    var documentIndex = 0
    val elements = document.getElementsByTagName("*")
    for (index in 0 until elements.length) {
        val element = elements.item(index) as? org.w3c.dom.Element ?: continue
        if (element.localName?.lowercase(Locale.ROOT) !in NativeSvgParser.shapeTags) continue
        val currentDocumentIndex = documentIndex++
        val shape = shapesByDocumentIndex[currentDocumentIndex] ?: continue
        val additions = buildList {
            if (shape.fill != shape.originalEdit.fill) add("fill:${shape.fill}")
            if (shape.stroke != shape.originalEdit.stroke) add("stroke:${shape.stroke}")
            if (shape.deleted) add("display:none")
        }
        if (additions.isNotEmpty()) {
            val originalStyle = element.getAttribute("style").trim().trimEnd(';')
            element.setAttribute(
                "style",
                (listOf(originalStyle).filter(String::isNotBlank) + additions).joinToString(";"),
            )
        }
    }
    val output = StringWriter()
    TransformerFactory.newInstance().newTransformer().apply {
        setOutputProperty(OutputKeys.OMIT_XML_DECLARATION, "yes")
    }.transform(DOMSource(document), StreamResult(output))
    return output.toString()
}

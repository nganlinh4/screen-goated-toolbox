@file:OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package dev.screengoated.toolbox.mobile.creation

import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Region
import android.graphics.Color as AndroidColor
import android.util.Xml
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.core.graphics.PathParser
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.i18n.CreationCommonLocale
import dev.screengoated.toolbox.mobile.ui.i18n.CreationSvgLocale
import java.io.StringReader
import java.util.Locale
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.hypot
import kotlin.math.min
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.xmlpull.v1.XmlPullParser

internal class CreationSvgDocumentController {
    internal var document by mutableStateOf<NativeSvgDocument?>(null)
        private set
    internal var revision by mutableIntStateOf(0)
        private set
    internal var zoom by mutableFloatStateOf(1f)
        private set
    internal var pan by mutableStateOf(Offset.Zero)
        private set
    internal var selectedIndex by mutableStateOf<Int?>(null)
        private set

    private val undo = ArrayDeque<SvgSnapshot>()
    private val redo = ArrayDeque<SvgSnapshot>()

    internal fun attach(value: NativeSvgDocument) {
        if (document === value) return
        document = value
        selectedIndex = null
        undo.clear()
        redo.clear()
        fit()
    }

    internal fun transform(panChange: Offset, zoomChange: Float) {
        zoom = (zoom * zoomChange).coerceIn(0.25f, 8f)
        pan = if (zoom <= 1f) Offset.Zero else pan + panChange
    }

    internal fun select(index: Int?) {
        selectedIndex = index
        revision += 1
    }

    fun fit() {
        zoom = 1f
        pan = Offset.Zero
    }

    fun zoomIn() { zoom = (zoom * 1.2f).coerceAtMost(8f) }
    fun zoomOut() {
        zoom = (zoom / 1.2f).coerceAtLeast(0.25f)
        if (zoom <= 1f) pan = Offset.Zero
    }

    fun undo() {
        val value = document ?: return
        val snapshot = undo.removeLastOrNull() ?: return
        redo.addLast(value.snapshot(selectedIndex))
        value.restore(snapshot)
        selectedIndex = snapshot.selected
        revision += 1
    }

    fun redo() {
        val value = document ?: return
        val snapshot = redo.removeLastOrNull() ?: return
        undo.addLast(value.snapshot(selectedIndex))
        value.restore(snapshot)
        selectedIndex = snapshot.selected
        revision += 1
    }

    fun deleteSelected() = mutate { shape -> shape.deleted = true }
    fun setFill(value: String) = mutate { shape -> shape.fill = value }
    fun setStroke(value: String) = mutate { shape -> shape.stroke = value }

    suspend fun serialize(): String = document?.serialize().orEmpty()

    internal fun destroy() {
        document = null
        undo.clear()
        redo.clear()
    }

    private fun mutate(action: (NativeSvgShape) -> Unit) {
        val value = document ?: return
        val shape = selectedIndex?.let(value.shapes::getOrNull) ?: return
        undo.addLast(value.snapshot(selectedIndex))
        while (undo.size > 40) undo.removeFirst()
        redo.clear()
        action(shape)
        revision += 1
    }
}

@Composable
internal fun CreationSvgDocument(
    outputPath: String,
    viewModel: CreationNativeViewModel,
    controller: CreationSvgDocumentController,
    modifier: Modifier = Modifier,
) {
    val document by produceState<NativeSvgDocument?>(null, outputPath) {
        value = runCatching {
            val svg = viewModel.readSvg(outputPath)
            withContext(Dispatchers.Default) { NativeSvgParser.parse(svg) }
        }.getOrNull()
    }
    val value = document
    if (value == null) {
        Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            androidx.compose.material3.CircularProgressIndicator()
        }
        return
    }
    LaunchedEffect(value) { controller.attach(value) }
    val accent = MaterialTheme.colorScheme.primary
    val revision = controller.revision
    val zoom = controller.zoom
    val pan = controller.pan
    val selected = controller.selectedIndex
    val checkerLight = MaterialTheme.colorScheme.surfaceContainerLowest
    val checkerDark = MaterialTheme.colorScheme.surfaceContainerHigh

    Canvas(
        modifier = modifier
            .fillMaxSize()
            .pointerInput(value, zoom, pan) {
                detectTapGestures { point ->
                    val transform = value.viewportTransform(size.width.toFloat(), size.height.toFloat(), zoom, pan)
                    controller.select(value.hitTest(transform.toDocument(point), transform.documentTolerance(8f)))
                }
            }
            .pointerInput(value) {
                detectTransformGestures { _, panChange, zoomChange, _ ->
                    controller.transform(panChange, zoomChange)
                }
            },
    ) {
        @Suppress("UNUSED_VARIABLE") val redraw = revision
        drawRect(checkerLight)
        val checkerSize = 10.dp.toPx()
        val columns = ceil(size.width / checkerSize).toInt()
        val rows = ceil(size.height / checkerSize).toInt()
        repeat(rows) { row ->
            repeat(columns) { column ->
                if ((row + column) % 2 == 0) {
                    drawRect(
                        color = checkerDark,
                        topLeft = Offset(column * checkerSize, row * checkerSize),
                        size = Size(checkerSize, checkerSize),
                    )
                }
            }
        }
        val transform = value.viewportTransform(size.width, size.height, zoom, pan)
        drawIntoCanvas { composeCanvas ->
            val canvas = composeCanvas.nativeCanvas
            canvas.save()
            canvas.translate(transform.origin.x + pan.x, transform.origin.y + pan.y)
            canvas.scale(transform.scale, transform.scale)
            canvas.translate(-value.viewBox.left, -value.viewBox.top)
            value.shapes.forEachIndexed { index, shape ->
                if (shape.deleted) return@forEachIndexed
                shape.draw(canvas)
                if (selected == index) {
                    val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                        style = Paint.Style.STROKE
                        color = accent.toArgb()
                        strokeWidth = 2.2f / transform.scale
                    }
                    canvas.drawPath(shape.path, paint)
                }
            }
            canvas.restore()
        }
    }
}

@Composable
internal fun CreationSvgEditorControls(
    controller: CreationSvgDocumentController,
    common: CreationCommonLocale,
    strings: CreationSvgLocale,
    accent: Color,
    onSave: () -> Unit,
) {
    val swatches = listOf(
        "none" to Color.Transparent,
        "#111111" to Color(0xff111111),
        "#ffffff" to Color.White,
        "#1976d2" to Color(0xff1976d2),
        "#00a38c" to Color(0xff00a38c),
        "#e14d72" to Color(0xffe14d72),
        "#f4b400" to Color(0xfff4b400),
    )
    androidx.compose.foundation.layout.Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        FlowRow(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            ViewerIconButton(R.drawable.ms_open_in_full, strings.fit, controller::fit)
            ViewerIconButton(R.drawable.ms_remove, strings.zoomOut, controller::zoomOut)
            ViewerIconButton(R.drawable.ms_add, strings.zoomIn, controller::zoomIn)
            ViewerIconButton(R.drawable.ms_arrow_back, strings.undo, controller::undo)
            ViewerIconButton(R.drawable.ms_arrow_forward, strings.redo, controller::redo)
            ViewerIconButton(R.drawable.ms_delete, common.delete, controller::deleteSelected)
            FilledTonalButton(onClick = onSave) {
                Icon(painterResource(R.drawable.ms_check), null, Modifier.size(18.dp))
                androidx.compose.foundation.layout.Spacer(Modifier.size(6.dp))
                Text(strings.saveEdits)
            }
        }
        PaintSwatches(strings.fill, swatches, accent) { controller.setFill(it) }
        PaintSwatches(strings.stroke, swatches, accent) { controller.setStroke(it) }
    }
}

@Composable
private fun ViewerIconButton(icon: Int, label: String, action: () -> Unit) {
    IconButton(onClick = action, modifier = Modifier.size(40.dp)) {
        Icon(painterResource(icon), contentDescription = label)
    }
}

@Composable
private fun PaintSwatches(
    label: String,
    swatches: List<Pair<String, Color>>,
    accent: Color,
    onSelect: (String) -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(label, style = MaterialTheme.typography.labelMedium, modifier = Modifier.size(48.dp, 24.dp))
        FlowRow(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(7.dp)) {
            swatches.forEach { (value, color) ->
                Box(
                    Modifier
                        .size(25.dp)
                        .background(if (color == Color.Transparent) MaterialTheme.colorScheme.surface else color, CircleShape)
                        .border(1.dp, if (color == Color.Transparent) accent else MaterialTheme.colorScheme.outlineVariant, CircleShape)
                        .clickable { onSelect(value) },
                )
            }
        }
    }
}

internal data class NativeSvgDocument(
    val viewBox: RectF,
    val width: String?,
    val height: String?,
    val definitions: String,
    val shapes: MutableList<NativeSvgShape>,
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

    fun snapshot(selected: Int?) = SvgSnapshot(
        shapes.map { SvgShapeEdit(it.fill, it.stroke, it.deleted) },
        selected,
    )

    fun restore(snapshot: SvgSnapshot) {
        shapes.zip(snapshot.shapes).forEach { (shape, edit) ->
            shape.fill = edit.fill
            shape.stroke = edit.stroke
            shape.deleted = edit.deleted
        }
    }

    fun serialize(): String = buildString {
        append("<svg xmlns=\"http://www.w3.org/2000/svg\"")
        width?.let { append(" width=\"").append(xmlEscape(it)).append('"') }
        height?.let { append(" height=\"").append(xmlEscape(it)).append('"') }
        append(" viewBox=\"").append(viewBox.left).append(' ').append(viewBox.top).append(' ')
            .append(viewBox.width()).append(' ').append(viewBox.height()).append("\">")
        append(definitions)
        shapes.filterNot { it.deleted }.forEach { append(it.serialize()) }
        append("</svg>")
    }
}

internal data class NativeSvgShape(
    val tag: String,
    val geometry: Map<String, String>,
    val matrix: Matrix,
    val path: Path,
    var fill: String,
    var stroke: String,
    val strokeWidth: Float,
    val opacity: Float,
    var deleted: Boolean = false,
) {
    fun draw(canvas: android.graphics.Canvas) {
        svgColor(fill)?.let { color ->
            canvas.drawPath(path, Paint(Paint.ANTI_ALIAS_FLAG).apply {
                style = Paint.Style.FILL
                this.color = color
                alpha = (AndroidColor.alpha(color) * opacity).toInt().coerceIn(0, 255)
            })
        }
        svgColor(stroke)?.let { color ->
            canvas.drawPath(path, Paint(Paint.ANTI_ALIAS_FLAG).apply {
                style = Paint.Style.STROKE
                strokeJoin = Paint.Join.ROUND
                strokeCap = Paint.Cap.ROUND
                this.color = color
                alpha = (AndroidColor.alpha(color) * opacity).toInt().coerceIn(0, 255)
                this.strokeWidth = strokeWidth.coerceAtLeast(0.25f)
            })
        }
    }

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

    fun serialize(): String = buildString {
        append('<').append(tag)
        geometry.forEach { (name, value) ->
            append(' ').append(name).append("=\"").append(xmlEscape(value)).append('"')
        }
        val values = FloatArray(9).also(matrix::getValues)
        if (!matrix.isIdentity) {
            append(" transform=\"matrix(")
                .append(values[Matrix.MSCALE_X]).append(' ')
                .append(values[Matrix.MSKEW_Y]).append(' ')
                .append(values[Matrix.MSKEW_X]).append(' ')
                .append(values[Matrix.MSCALE_Y]).append(' ')
                .append(values[Matrix.MTRANS_X]).append(' ')
                .append(values[Matrix.MTRANS_Y]).append(")\"")
        }
        append(" fill=\"").append(xmlEscape(fill)).append('"')
        append(" stroke=\"").append(xmlEscape(stroke)).append('"')
        append(" stroke-width=\"").append(strokeWidth).append('"')
        if (opacity < 1f) append(" opacity=\"").append(opacity).append('"')
        append("/>")
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

internal data class SvgSnapshot(val shapes: List<SvgShapeEdit>, val selected: Int?)
internal data class SvgShapeEdit(val fill: String, val stroke: String, val deleted: Boolean)

private object NativeSvgParser {
    private val shapeTags = setOf("path", "rect", "circle", "ellipse", "line", "polyline", "polygon")
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
                    if (tag in shapeTags) createShape(tag, parser, matrix, style)?.let(shapes::add)
                }
                XmlPullParser.END_TAG -> {
                    matrices.removeLastOrNull()
                    styles.removeLastOrNull()
                }
            }
            event = parser.next()
        }
        require(shapes.isNotEmpty()) { "SVG contains no supported vector paths" }
        val definitions = Regex("<defs(?:\\s[^>]*)?>[\\s\\S]*?</defs>", RegexOption.IGNORE_CASE)
            .find(svg)?.value.orEmpty()
        return NativeSvgDocument(viewBox, width, height, definitions, shapes)
    }

    private fun createShape(tag: String, parser: XmlPullParser, matrix: Matrix, style: SvgStyle): NativeSvgShape? {
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

private fun svgColor(value: String): Int? {
    val clean = value.trim()
    if (clean.equals("none", true) || clean.startsWith("url(", true)) return null
    if (clean.startsWith("rgb(", true)) {
        val values = Regex("[\\d.]+").findAll(clean).map { it.value.toFloat() }.toList()
        if (values.size >= 3) return AndroidColor.rgb(values[0].toInt(), values[1].toInt(), values[2].toInt())
    }
    return runCatching { AndroidColor.parseColor(clean) }.getOrNull()
}

private fun xmlEscape(value: String): String = value
    .replace("&", "&amp;")
    .replace("\"", "&quot;")
    .replace("<", "&lt;")
    .replace(">", "&gt;")

@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, androidx.compose.ui.text.ExperimentalTextApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock
import kotlin.math.roundToInt

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

data class NodePosition(
    val id: String,
    val x: Float,
    val y: Float,
    val block: ProcessingBlock,
)

data class Connection(
    val fromNodeId: String,
    val toNodeId: String,
)

data class NodeGraphState(
    val nodes: List<NodePosition>,
    val connections: List<Connection>,
)

// ---------------------------------------------------------------------------
// Sizing / layout constants (dp)
// ---------------------------------------------------------------------------

private val NODE_WIDTH_DP = 220.dp
private val PIN_RADIUS_DP = 7.dp
private val PIN_HIT_RADIUS_DP = 30.dp
private val GRID_SPACING_DP = 24.dp
private const val GRID_DOT_RADIUS = 1.5f
private const val MIN_ZOOM = 0.25f
private const val MAX_ZOOM = 2.5f
private const val DEFAULT_NODE_HEIGHT_PX = 600f // generous estimate for nodes with prompts
private const val FIT_PADDING = 40f // px padding around auto-fit content

// Layout gap between nodes (fraction of node dimension)
private const val LAYOUT_GAP_X_RATIO = 0.6f  // horizontal gap = 60% of node width
private const val LAYOUT_GAP_Y_RATIO = 0.25f // vertical gap = 25% of node height

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

// Node colors use Material 3 dynamic accent (Material You) —
// 3 tonal variants from the device accent color.
// These are @Composable getters since they read MaterialTheme.colorScheme.

private data class NodeColors(
    val bg: Color,
    val title: Color,
    val content: Color,
    val pill: Color,
)

/** Google Sans Flex at wdth=75 for condensed descriptive text. */
private val condensedFontFamily: androidx.compose.ui.text.font.FontFamily by lazy {
    if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
        androidx.compose.ui.text.font.FontFamily(
            androidx.compose.ui.text.font.Font(
                resId = dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
                variationSettings = androidx.compose.ui.text.font.FontVariation.Settings(
                    androidx.compose.ui.text.font.FontVariation.Setting("wdth", 75f),
                ),
            ),
        )
    } else {
        androidx.compose.ui.text.font.FontFamily.Default
    }
}

/** All ISO 639-1 language names, sorted — matches Windows get_all_languages() from isolang crate. */
private val ALL_ISO_LANGUAGES: List<String> by lazy {
    java.util.Locale.getISOLanguages()
        .mapNotNull { code ->
            val loc = java.util.Locale(code)
            loc.getDisplayLanguage(java.util.Locale.ENGLISH).takeIf { it.isNotBlank() && it != code }
        }
        .distinct()
        .sorted()
}

// Material Symbol: file_copy (rounded, 24px)
// SVG path from viewBox="0 -960 960 960", all Y coords shifted by +960
private val FileCopyIcon: androidx.compose.ui.graphics.vector.ImageVector by lazy {
    androidx.compose.ui.graphics.vector.ImageVector.Builder(
        name = "FileCopy", defaultWidth = 24.dp, defaultHeight = 24.dp,
        viewportWidth = 960f, viewportHeight = 960f,
    ).apply {
        addPath(
            pathData = androidx.compose.ui.graphics.vector.PathData {
                // M760-200 → M760,760
                moveTo(760f, 760f); horizontalLineTo(320f)
                quadTo(287f, 760f, 263.5f, 736.5f); quadTo(240f, 713f, 240f, 680f)
                verticalLineTo(120f)
                quadTo(240f, 87f, 263.5f, 63.5f); quadTo(287f, 40f, 320f, 40f)
                horizontalLineTo(600f); lineTo(840f, 280f); verticalLineTo(680f)
                quadTo(840f, 713f, 816.5f, 736.5f); quadTo(793f, 760f, 760f, 760f)
                close()
                moveTo(560f, 320f); verticalLineTo(120f); horizontalLineTo(320f)
                verticalLineTo(680f); horizontalLineTo(760f); verticalLineTo(320f)
                horizontalLineTo(560f); close()
                moveTo(160f, 920f)
                quadTo(127f, 920f, 103.5f, 896.5f); quadTo(80f, 873f, 80f, 840f)
                verticalLineTo(280f); horizontalLineTo(160f); verticalLineTo(840f)
                horizontalLineTo(600f); verticalLineTo(920f); horizontalLineTo(160f)
                close()
                moveTo(320f, 120f); verticalLineTo(320f); verticalLineTo(120f)
                verticalLineTo(680f); verticalLineTo(120f); close()
            },
            fill = androidx.compose.ui.graphics.SolidColor(Color.Black),
        )
    }.build()
}

// Material Symbol: file_copy_off (rounded, 24px)
// SVG path from viewBox="0 -960 960 960", all Y coords shifted by +960
private val FileCopyOffIcon: androidx.compose.ui.graphics.vector.ImageVector by lazy {
    androidx.compose.ui.graphics.vector.ImageVector.Builder(
        name = "FileCopyOff", defaultWidth = 24.dp, defaultHeight = 24.dp,
        viewportWidth = 960f, viewportHeight = 960f,
    ).apply {
        addPath(
            pathData = androidx.compose.ui.graphics.vector.PathData {
                moveTo(840f, 726f); lineTo(760f, 646f); verticalLineTo(320f)
                horizontalLineTo(560f); verticalLineTo(120f); horizontalLineTo(320f)
                verticalLineTo(206f); lineTo(240f, 126f); verticalLineTo(120f)
                quadTo(240f, 87f, 263.5f, 63.5f); quadTo(287f, 40f, 320f, 40f)
                horizontalLineTo(600f); lineTo(840f, 280f); verticalLineTo(726f)
                close()
                moveTo(320f, 680f); horizontalLineTo(568f); lineTo(320f, 432f)
                verticalLineTo(680f); close()
                moveTo(820f, 932f); lineTo(648f, 760f); horizontalLineTo(320f)
                quadTo(287f, 760f, 263.5f, 736.5f); quadTo(240f, 713f, 240f, 680f)
                verticalLineTo(352f); lineTo(28f, 140f); lineTo(84f, 84f)
                lineTo(876f, 876f); lineTo(820f, 932f); close()
                moveTo(540f, 383f); close()
                moveTo(444f, 556f); close()
                moveTo(160f, 920f)
                quadTo(127f, 920f, 103.5f, 896.5f); quadTo(80f, 873f, 80f, 840f)
                verticalLineTo(320f); horizontalLineTo(160f); verticalLineTo(840f)
                horizontalLineTo(640f); verticalLineTo(920f); horizontalLineTo(160f)
                close()
            },
            fill = androidx.compose.ui.graphics.SolidColor(Color.Black),
        )
    }.build()
}

private val PIN_INPUT_COLOR = Color(0xFF66BB6A)
private val PIN_OUTPUT_COLOR = Color(0xFF42A5F5)
private val WIRE_DRAG_COLOR = Color(0xFFFFAB40)

// ---------------------------------------------------------------------------
// BFS layout for nodes with no stored position
// ---------------------------------------------------------------------------

/**
 * BFS-based layer layout matching the Windows `blocks_to_snarl` algorithm.
 * Each layer is vertically centered. Spacing is proportional to actual node size.
 *
 * @param nodeWidthPx  actual node card width in px
 * @param nodeHeightPx estimated node card height in px
 */
internal fun bfsLayout(
    nodes: List<NodePosition>,
    connections: List<Connection>,
    nodeWidthPx: Float,
    nodeHeightPx: Float,
): List<NodePosition> {
    if (nodes.isEmpty()) return nodes

    val spacingX = nodeWidthPx * (1f + LAYOUT_GAP_X_RATIO)
    val spacingY = nodeHeightPx * (1f + LAYOUT_GAP_Y_RATIO)
    val startX = nodeWidthPx * 0.15f

    // Build adjacency
    val adjacency = mutableMapOf<String, MutableList<String>>()
    nodes.forEach { adjacency[it.id] = mutableListOf() }
    connections.forEach { c -> adjacency[c.fromNodeId]?.add(c.toNodeId) }

    // BFS from root nodes
    val hasIncoming = connections.map { it.toNodeId }.toSet()
    val roots = nodes.filter { it.id !in hasIncoming }.map { it.id }
        .ifEmpty { listOf(nodes.first().id) }

    val depthOf = mutableMapOf<String, Int>()
    val visited = mutableSetOf<String>()
    val queue = ArrayDeque<String>()
    roots.forEach { r -> depthOf[r] = 0; visited.add(r); queue.add(r) }
    while (queue.isNotEmpty()) {
        val cur = queue.removeFirst()
        val nextDepth = (depthOf[cur] ?: 0) + 1
        adjacency[cur]?.forEach { neighbor ->
            if (neighbor !in visited) {
                visited.add(neighbor)
                depthOf[neighbor] = nextDepth
                queue.add(neighbor)
            }
        }
    }

    // Group by layer
    val layerNodes = mutableMapOf<Int, MutableList<NodePosition>>()
    nodes.forEach { node ->
        val depth = depthOf[node.id] ?: 0
        layerNodes.getOrPut(depth) { mutableListOf() }.add(node)
    }

    // Find total height to center everything
    val maxLayerCount = layerNodes.values.maxOfOrNull { it.size } ?: 1
    val centerY = (maxLayerCount * spacingY) / 2f

    val posMap = mutableMapOf<String, Offset>()

    // Assign positions — each layer vertically centered
    for ((depth, nodesInLayer) in layerNodes) {
        val count = nodesInLayer.size
        val layerHeight = count * spacingY
        val layerStartY = centerY - layerHeight / 2f + spacingY / 2f - nodeHeightPx / 2f

        nodesInLayer.forEachIndexed { i, node ->
            val x = startX + depth * spacingX
            val y = layerStartY + i * spacingY
            posMap[node.id] = Offset(x, y)
        }
    }

    // Fallback for unreachable nodes
    nodes.filter { it.id !in visited }.forEachIndexed { i, node ->
        posMap[node.id] = Offset(startX + i * spacingX, centerY + spacingY * 2f)
    }

    return nodes.map { node ->
        val pos = posMap[node.id] ?: Offset(node.x, node.y)
        node.copy(x = pos.x, y = pos.y)
    }
}

// ---------------------------------------------------------------------------
// Pin position helpers — use actual measured height per node
// ---------------------------------------------------------------------------

// Pin Y offset: header(5dp) + padding(6dp) + pin center(7dp) ≈ 18dp from node top
private const val PIN_Y_OFFSET_PX = 50f // ~18dp at typical density, will be scaled

private fun inputPinCenter(
    node: NodePosition,
    measuredHeights: Map<String, Float>,
): Offset {
    return Offset(node.x + 10f, node.y + PIN_Y_OFFSET_PX)
}

private fun outputPinCenter(
    node: NodePosition,
    nodeWidthPx: Float,
    measuredHeights: Map<String, Float>,
): Offset {
    return Offset(node.x + nodeWidthPx - 10f, node.y + PIN_Y_OFFSET_PX)
}

// ---------------------------------------------------------------------------
// Connection validation (matches Windows viewer.rs rules)
// ---------------------------------------------------------------------------

internal fun canConnect(
    fromNode: NodePosition,
    toNode: NodePosition,
    existingConnections: List<Connection>,
): Boolean {
    if (fromNode.id == toNode.id) return false
    if (toNode.block.blockType == BlockType.INPUT_ADAPTER) return false
    if (existingConnections.any { it.toNodeId == toNode.id }) return false
    val isSpecialTarget = toNode.block.blockType == BlockType.IMAGE ||
        toNode.block.blockType == BlockType.AUDIO
    if (isSpecialTarget && fromNode.block.blockType != BlockType.INPUT_ADAPTER) return false
    if (existingConnections.any { it.fromNodeId == fromNode.id && it.toNodeId == toNode.id }) return false
    return true
}

// ---------------------------------------------------------------------------
// Canvas drawing helpers
// ---------------------------------------------------------------------------

private fun DrawScope.drawGridDots(gridSpacingPx: Float, dotColor: Color, pan: Offset, zoom: Float) {
    val w = size.width; val h = size.height
    val step = gridSpacingPx * zoom
    if (step < 4f) return
    val offsetX = pan.x % step; val offsetY = pan.y % step
    var x = offsetX
    while (x < w) {
        var y = offsetY
        while (y < h) {
            drawCircle(dotColor, radius = GRID_DOT_RADIUS * zoom, center = Offset(x, y))
            y += step
        }
        x += step
    }
}

private fun DrawScope.drawBezierConnection(
    from: Offset, to: Offset, color: Color, strokeWidthPx: Float, alpha: Float = 1f,
) {
    val dx = (to.x - from.x).coerceAtLeast(40f) * 0.45f
    val path = Path().apply {
        moveTo(from.x, from.y)
        cubicTo(from.x + dx, from.y, to.x - dx, to.y, to.x, to.y)
    }
    drawPath(path, color.copy(alpha = alpha), style = Stroke(width = strokeWidthPx, cap = StrokeCap.Round))
}

private fun isNearBezier(point: Offset, from: Offset, to: Offset, threshold: Float): Boolean {
    val dx = (to.x - from.x).coerceAtLeast(40f) * 0.45f
    for (i in 0..20) {
        val t = i / 20f; val mt = 1f - t
        val x = mt * mt * mt * from.x + 3f * mt * mt * t * (from.x + dx) +
            3f * mt * t * t * (to.x - dx) + t * t * t * to.x
        val y = mt * mt * mt * from.y + 3f * mt * mt * t * from.y +
            3f * mt * t * t * to.y + t * t * t * to.y
        val dist = kotlin.math.sqrt((point.x - x) * (point.x - x) + (point.y - y) * (point.y - y))
        if (dist < threshold) return true
    }
    return false
}

// ---------------------------------------------------------------------------
// Localized node type label
// ---------------------------------------------------------------------------

private fun nodeTypeLabel(
    blockType: BlockType,
    lang: String,
    presetType: dev.screengoated.toolbox.mobile.shared.preset.PresetType =
        dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT,
): String = when (blockType) {
    BlockType.INPUT_ADAPTER -> {
        val inputSuffix = when (presetType) {
            dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE ->
                when (lang) { "vi" -> "Hình ảnh"; "ko" -> "이미지"; else -> "Image" }
            dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC,
            dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO ->
                when (lang) { "vi" -> "Âm thanh"; "ko" -> "오디오"; else -> "Audio" }
            else ->
                when (lang) { "vi" -> "Văn bản"; "ko" -> "텍스트"; else -> "Text" }
        }
        val prefix = when (lang) { "vi" -> "Đầu vào"; "ko" -> "입력"; else -> "Input" }
        "$prefix: $inputSuffix"
    }
    BlockType.TEXT -> when (lang) { "vi" -> "Text -> Text"; "ko" -> "텍스트 -> 텍스트"; else -> "Text -> Text" }
    BlockType.IMAGE -> when (lang) { "vi" -> "Ảnh -> Text"; "ko" -> "이미지 -> 텍스트"; else -> "Image -> Text" }
    BlockType.AUDIO -> when (lang) { "vi" -> "Audio -> Text"; "ko" -> "오디오 -> 텍스트"; else -> "Audio -> Text" }
}

// ---------------------------------------------------------------------------
// Node card composable
// ---------------------------------------------------------------------------

@Composable
private fun NodeCard(
    node: NodePosition,
    isSelected: Boolean,
    onTap: () -> Unit,
    onDrag: (dx: Float, dy: Float) -> Unit,
    onDragEnd: () -> Unit,
    onDelete: () -> Unit,
    onOutputPinDragStart: () -> Unit,
    onOutputPinDrag: (Offset) -> Unit,
    onOutputPinDragEnd: () -> Unit,
    onMeasured: (heightPx: Float) -> Unit,
    onBlockUpdated: (ProcessingBlock) -> Unit = {},
    onPromptEditRequest: () -> Unit = {},
    presetType: dev.screengoated.toolbox.mobile.shared.preset.PresetType =
        dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT,
    providerSettings: dev.screengoated.toolbox.mobile.preset.PresetProviderSettings =
        dev.screengoated.toolbox.mobile.preset.PresetProviderSettings(),
    modifier: Modifier = Modifier,
    lang: String = "en",
) {
    val block = node.block
    val colors = MaterialTheme.colorScheme

    // 3 accent-derived node styles via Material You dynamic color
    // secondary = lightest/most muted (common Text->Text nodes)
    // tertiary = mid tone (input adapter)
    // primary = boldest (rare special nodes)
    val (cardBg, titleCol, contentCol, pillBg) = when (block.blockType) {
        BlockType.INPUT_ADAPTER -> NodeColors(
            colors.tertiaryContainer, colors.onTertiaryContainer,
            colors.onTertiaryContainer.copy(alpha = 0.75f),
            colors.onTertiaryContainer.copy(alpha = 0.1f),
        )
        BlockType.TEXT -> NodeColors(
            colors.secondaryContainer, colors.onSecondaryContainer,
            colors.onSecondaryContainer.copy(alpha = 0.75f),
            colors.onSecondaryContainer.copy(alpha = 0.1f),
        )
        BlockType.IMAGE, BlockType.AUDIO -> NodeColors(
            colors.primaryContainer, colors.onPrimaryContainer,
            colors.onPrimaryContainer.copy(alpha = 0.75f),
            colors.onPrimaryContainer.copy(alpha = 0.1f),
        )
    }

    Card(
        modifier = modifier
            .width(NODE_WIDTH_DP)
            .onGloballyPositioned { coords -> onMeasured(coords.size.height.toFloat()) }
            .pointerInput(node.id) {
                detectTapGestures(
                    onTap = { onTap() },
                )
            }
            .pointerInput(node.id + "_drag") {
                detectDragGestures(
                    onDrag = { change, dragAmount ->
                        change.consume()
                        onDrag(dragAmount.x, dragAmount.y)
                    },
                    onDragEnd = { onDragEnd() },
                    onDragCancel = { onDragEnd() },
                )
            },
        colors = CardDefaults.cardColors(
            containerColor = cardBg,
        ),
        border = androidx.compose.foundation.BorderStroke(
            0.5.dp,
            titleCol.copy(alpha = 0.15f),
        ),
        shape = MaterialTheme.shapes.medium,
        elevation = CardDefaults.cardElevation(defaultElevation = 1.dp),
    ) {
        Column {
            Row(
                modifier = Modifier.padding(start = 10.dp, end = 10.dp, top = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                // Input pin (not on INPUT_ADAPTER)
                if (block.blockType != BlockType.INPUT_ADAPTER) {
                    Surface(
                        modifier = Modifier.size(PIN_RADIUS_DP * 2),
                        shape = CircleShape,
                        color = PIN_INPUT_COLOR,
                        content = {},
                    )
                    Spacer(Modifier.width(6.dp))
                }

                // Title
                Text(
                    text = nodeTypeLabel(block.blockType, lang, presetType),
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = titleCol,
                    modifier = Modifier.weight(1f),
                )

                // Delete button (not on input adapter)
                if (block.blockType != BlockType.INPUT_ADAPTER) {
                    Box(
                        modifier = Modifier
                            .size(20.dp)
                            .pointerInput(node.id + "_del") {
                                detectTapGestures { onDelete() }
                            },
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(
                            painter = painterResource(R.drawable.ms_close),
                            contentDescription = null,
                            modifier = Modifier.size(12.dp),
                            tint = contentCol.copy(alpha = 0.5f),
                        )
                    }
                }

                // Output pin
                Spacer(Modifier.width(6.dp))
                Box(
                    modifier = Modifier
                        .size(PIN_HIT_RADIUS_DP)
                        .pointerInput(node.id + "_pin") {
                            detectDragGestures(
                                onDragStart = { onOutputPinDragStart() },
                                onDrag = { change, dragAmount ->
                                    change.consume()
                                    onOutputPinDrag(dragAmount)
                                },
                                onDragEnd = { onOutputPinDragEnd() },
                                onDragCancel = { onOutputPinDragEnd() },
                            )
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Surface(
                        modifier = Modifier.size(PIN_RADIUS_DP * 2),
                        shape = CircleShape,
                        color = PIN_OUTPUT_COLOR,
                        content = {},
                    )
                }
            }

            // Always-visible inline editor (like Windows)
            Column(
                modifier = Modifier.padding(horizontal = 10.dp).padding(bottom = 8.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                if (block.blockType != BlockType.INPUT_ADAPTER) {
                    // Row 1: "Mô hình:" label + model dropdown (same row)
                    var showModelDropdown by remember { mutableStateOf(false) }
                    val catalog = dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
                    val descriptor = catalog.getById(block.model)
                    val isNonLlm = descriptor?.isNonLlm == true
                    val availableModels = remember(block.blockType, providerSettings) {
                        catalog.forBlockType(block.blockType).filter { model ->
                            when (model.provider) {
                                dev.screengoated.toolbox.mobile.preset.PresetModelProvider.GROQ -> providerSettings.useGroq
                                dev.screengoated.toolbox.mobile.preset.PresetModelProvider.GOOGLE,
                                dev.screengoated.toolbox.mobile.preset.PresetModelProvider.GEMINI_LIVE,
                                -> providerSettings.useGemini
                                dev.screengoated.toolbox.mobile.preset.PresetModelProvider.OPENROUTER -> providerSettings.useOpenRouter
                                dev.screengoated.toolbox.mobile.preset.PresetModelProvider.CEREBRAS -> providerSettings.useCerebras
                                dev.screengoated.toolbox.mobile.preset.PresetModelProvider.OLLAMA -> providerSettings.useOllama
                                else -> true
                            }
                        }
                    }

                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            text = when (lang) { "vi" -> "Mô hình:"; "ko" -> "모델:"; else -> "Model:" },
                            style = MaterialTheme.typography.labelSmall,
                            color = contentCol.copy(alpha = 0.6f),
                        )
                        Spacer(Modifier.width(4.dp))
                        Box {
                            Surface(
                                modifier = Modifier
                                    .pointerInput(Unit) { detectTapGestures { showModelDropdown = true } },
                                shape = RoundedCornerShape(4.dp),
                                color = pillBg,
                            ) {
                                Text(
                                    text = descriptor?.localizedName(lang) ?: block.model,
                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                                    style = MaterialTheme.typography.labelSmall,
                                    color = contentCol,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                            androidx.compose.material3.DropdownMenu(
                                expanded = showModelDropdown,
                                onDismissRequest = { showModelDropdown = false },
                                modifier = Modifier.widthIn(min = 300.dp),
                            ) {
                                availableModels.forEach { model ->
                                    val providerIcon = when (model.provider) {
                                        dev.screengoated.toolbox.mobile.preset.PresetModelProvider.GOOGLE,
                                        dev.screengoated.toolbox.mobile.preset.PresetModelProvider.GEMINI_LIVE,
                                        -> R.drawable.ms_auto_awesome
                                        dev.screengoated.toolbox.mobile.preset.PresetModelProvider.GOOGLE_GTX -> R.drawable.ms_translate
                                        dev.screengoated.toolbox.mobile.preset.PresetModelProvider.GROQ -> R.drawable.ms_bolt
                                        dev.screengoated.toolbox.mobile.preset.PresetModelProvider.CEREBRAS -> R.drawable.ms_local_fire_department
                                        dev.screengoated.toolbox.mobile.preset.PresetModelProvider.OPENROUTER -> R.drawable.ms_public
                                        dev.screengoated.toolbox.mobile.preset.PresetModelProvider.OLLAMA -> R.drawable.ms_computer
                                        dev.screengoated.toolbox.mobile.preset.PresetModelProvider.TAALAS -> R.drawable.ms_auto_awesome
                                        else -> R.drawable.ms_auto_awesome
                                    }
                                    val hasSearch = catalog.supportsSearchById(model.id)
                                    val isSelected = model.id == block.model
                                    androidx.compose.material3.DropdownMenuItem(
                                        modifier = if (isSelected) Modifier
                                            .padding(horizontal = 4.dp)
                                            .background(
                                                MaterialTheme.colorScheme.primary.copy(alpha = 0.08f),
                                                RoundedCornerShape(8.dp),
                                            )
                                        else Modifier,
                                        leadingIcon = {
                                            Icon(painterResource(providerIcon), null, modifier = Modifier.size(16.dp))
                                        },
                                        trailingIcon = {
                                            if (hasSearch) {
                                                Icon(
                                                    painterResource(R.drawable.ms_search),
                                                    null,
                                                    modifier = Modifier.size(14.dp),
                                                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                                                )
                                            }
                                        },
                                        text = {
                                            val quota = model.localizedQuota(lang)
                                            val suffix = if (quota.isNotBlank()) " - ${model.fullName} - $quota"
                                                else " - ${model.fullName}"
                                            Text(
                                                text = androidx.compose.ui.text.buildAnnotatedString {
                                                    pushStyle(androidx.compose.ui.text.SpanStyle(
                                                        fontWeight = if (isSelected) FontWeight.Bold else FontWeight.SemiBold,
                                                    ))
                                                    append(model.localizedName(lang))
                                                    pop()
                                                    pushStyle(androidx.compose.ui.text.SpanStyle(
                                                        fontSize = 11.sp,
                                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                                        fontFamily = condensedFontFamily,
                                                    ))
                                                    append(suffix)
                                                    pop()
                                                },
                                                style = MaterialTheme.typography.bodySmall,
                                            )
                                        },
                                        onClick = {
                                            onBlockUpdated(block.copy(model = model.id))
                                            showModelDropdown = false
                                        },
                                    )
                                }
                            }
                        }
                    }

                    // Row 2: "Lệnh:" label + "+ Ngôn ngữ" button (only for LLM models)
                    if (!isNonLlm) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                text = when (lang) { "vi" -> "Lệnh:"; "ko" -> "프롬프트:"; else -> "Prompt:" },
                                style = MaterialTheme.typography.labelSmall,
                                color = contentCol.copy(alpha = 0.6f),
                            )
                            Spacer(Modifier.weight(1f))
                            // "+ Ngôn ngữ" button
                            Surface(
                                modifier = Modifier
                                    .pointerInput(node.id + "_addlang") {
                                        detectTapGestures {
                                            // Find next available language slot (max 10)
                                            val existing = block.languageVars.keys
                                                .mapNotNull { it.removePrefix("language").toIntOrNull() }
                                                .toSet()
                                            val nextN = (1..10).firstOrNull { it !in existing } ?: return@detectTapGestures
                                            val newKey = "language$nextN"
                                            val newPrompt = block.prompt + " {$newKey}"
                                            val newVars = block.languageVars + (newKey to "Vietnamese")
                                            onBlockUpdated(block.copy(prompt = newPrompt, languageVars = newVars))
                                        }
                                    },
                                shape = RoundedCornerShape(8.dp),
                                color = Color(0xFF5A8A90).copy(alpha = 0.8f),
                            ) {
                                Text(
                                    text = when (lang) { "vi" -> "+ Ngôn ngữ"; "ko" -> "+ 언어"; else -> "+ Language" },
                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp),
                                    style = MaterialTheme.typography.labelSmall,
                                    color = Color.White,
                                )
                            }
                        }
                    }

                    // Row 3: Prompt text preview
                    if (!isNonLlm) {
                    Surface(
                        modifier = Modifier
                            .fillMaxWidth()
                            .pointerInput(node.id + "_prompt") {
                                detectTapGestures { onPromptEditRequest() }
                            },
                        shape = RoundedCornerShape(6.dp),
                        color = pillBg,
                    ) {
                        Text(
                            text = block.prompt.ifBlank { "Prompt…" },
                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 6.dp),
                            style = MaterialTheme.typography.bodySmall,
                            color = if (block.prompt.isBlank())
                                contentCol.copy(alpha = 0.4f)
                            else contentCol,
                            maxLines = 4,
                            overflow = TextOverflow.Ellipsis,
                            lineHeight = 14.sp,
                        )
                    }
                    } // end if (!isNonLlm) for prompt

                    // Row 4+: Language variable rows — ONLY for tags found in prompt
                    // (matches Windows utils.rs: scan prompt for {languageN}, ignore stale map entries)
                    if (!isNonLlm) {
                        val detectedVars = (1..10).filter { n ->
                            block.prompt.contains("{language$n}")
                        }
                        detectedVars.forEach { num ->
                            val key = "language$num"
                            // Auto-insert default if tag exists but no map entry
                            val currentValue = block.languageVars[key] ?: run {
                                val newVars = block.languageVars + (key to "Vietnamese")
                                onBlockUpdated(block.copy(languageVars = newVars))
                                "Vietnamese"
                            }
                            var showLangDropdown by remember { mutableStateOf(false) }
                            var langSearchQuery by remember { mutableStateOf("") }
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    text = "{$key}:",
                                    style = MaterialTheme.typography.labelSmall,
                                    color = contentCol.copy(alpha = 0.5f),
                                )
                                Spacer(Modifier.width(4.dp))
                                Box {
                                    Surface(
                                        modifier = Modifier
                                            .pointerInput(key) { detectTapGestures { showLangDropdown = true } },
                                        shape = RoundedCornerShape(8.dp),
                                        color = Color(0xFF6E5AAF).copy(alpha = 0.25f),
                                    ) {
                                        Text(
                                            text = currentValue,
                                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp),
                                            style = MaterialTheme.typography.labelSmall,
                                            fontWeight = FontWeight.SemiBold,
                                            color = contentCol,
                                        )
                                    }
                                    androidx.compose.material3.DropdownMenu(
                                        expanded = showLangDropdown,
                                        onDismissRequest = {
                                            showLangDropdown = false
                                            langSearchQuery = ""
                                        },
                                        modifier = Modifier.widthIn(min = 200.dp),
                                        properties = androidx.compose.ui.window.PopupProperties(focusable = true),
                                    ) {
                                        // Sticky search box
                                        androidx.compose.material3.OutlinedTextField(
                                            value = langSearchQuery,
                                            onValueChange = { langSearchQuery = it },
                                            modifier = Modifier
                                                .fillMaxWidth()
                                                .padding(horizontal = 8.dp, vertical = 4.dp),
                                            placeholder = { Text("Search...", style = MaterialTheme.typography.bodySmall) },
                                            singleLine = true,
                                            textStyle = MaterialTheme.typography.bodySmall,
                                        )
                                        androidx.compose.material3.HorizontalDivider()
                                        // Scrollable language list
                                        val filteredLangs = remember(langSearchQuery) {
                                            val query = langSearchQuery.lowercase()
                                            ALL_ISO_LANGUAGES.filter {
                                                query.isEmpty() || it.lowercase().contains(query)
                                            }
                                        }
                                        Column(
                                            modifier = Modifier
                                                .heightIn(max = 250.dp)
                                                .verticalScroll(rememberScrollState()),
                                        ) {
                                        filteredLangs.forEach { language ->
                                            androidx.compose.material3.DropdownMenuItem(
                                                text = {
                                                    Text(
                                                        language,
                                                        style = MaterialTheme.typography.bodySmall,
                                                        fontWeight = if (language == currentValue) FontWeight.Bold else FontWeight.Normal,
                                                    )
                                                },
                                                onClick = {
                                                    val newVars = block.languageVars.toMutableMap()
                                                    newVars[key] = language
                                                    onBlockUpdated(block.copy(languageVars = newVars))
                                                    showLangDropdown = false
                                                    langSearchQuery = ""
                                                },
                                            )
                                        }
                                        } // end Column (scrollable)
                                    }
                                }
                            }
                        }
                    }

                    // Bottom icon toolbar row
                    var showRenderModeMenu by remember { mutableStateOf(false) }
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(2.dp),
                    ) {
                        // Eye toggle
                        androidx.compose.material3.IconToggleButton(
                            checked = block.showOverlay,
                            onCheckedChange = { onBlockUpdated(block.copy(showOverlay = it)) },
                            modifier = Modifier.size(24.dp),
                        ) {
                            Icon(
                                painter = painterResource(if (block.showOverlay) R.drawable.ms_visibility else R.drawable.ms_visibility_off),
                                contentDescription = null,
                                modifier = Modifier.size(14.dp),
                            )
                        }

                        // Stream mode toggle pill (mobile always uses markdown)
                        if (block.showOverlay) {
                            val isStreaming = block.streamingEnabled
                            val streamLabel = when (lang) {
                                "vi" -> if (isStreaming) "Stream" else "Không stream"
                                "ko" -> if (isStreaming) "스트림" else "스트림 없음"
                                else -> if (isStreaming) "Stream" else "No Stream"
                            }
                            Box {
                                Surface(
                                    shape = RoundedCornerShape(4.dp),
                                    color = pillBg,
                                    modifier = Modifier.height(20.dp)
                                        .pointerInput(Unit) { detectTapGestures { showRenderModeMenu = true } },
                                ) {
                                    Text(
                                        streamLabel,
                                        modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                        style = MaterialTheme.typography.labelSmall,
                                        fontSize = 9.sp,
                                        color = contentCol,
                                    )
                                }
                                androidx.compose.material3.DropdownMenu(
                                    expanded = showRenderModeMenu,
                                    onDismissRequest = { showRenderModeMenu = false },
                                ) {
                                    val streamOff = when (lang) { "vi" -> "Không stream"; "ko" -> "스트림 없음"; else -> "No Stream" }
                                    val streamOn = "Stream"
                                    listOf(
                                        streamOff to false,
                                        streamOn to true,
                                    ).forEach { (label, streaming) ->
                                        androidx.compose.material3.DropdownMenuItem(
                                            text = { Text(label, style = MaterialTheme.typography.bodySmall) },
                                            onClick = {
                                                val mode = if (streaming) "markdown_stream" else "markdown"
                                                onBlockUpdated(block.copy(renderMode = mode, streamingEnabled = streaming))
                                                showRenderModeMenu = false
                                            },
                                        )
                                    }
                                }
                            }
                        }

                        Spacer(Modifier.weight(1f))

                        // Copy toggle (distinct icons for on/off like eye)
                        androidx.compose.material3.IconToggleButton(
                            checked = block.autoCopy,
                            onCheckedChange = { onBlockUpdated(block.copy(autoCopy = it)) },
                            modifier = Modifier.size(24.dp),
                        ) {
                            Icon(
                                imageVector = if (block.autoCopy) FileCopyIcon
                                    else FileCopyOffIcon,
                                contentDescription = null,
                                modifier = Modifier.size(14.dp),
                            )
                        }

                        // Speak toggle (distinct icons for on/off like eye)
                        androidx.compose.material3.IconToggleButton(
                            checked = block.autoSpeak,
                            onCheckedChange = { onBlockUpdated(block.copy(autoSpeak = it)) },
                            modifier = Modifier.size(24.dp),
                        ) {
                            Icon(
                                painter = painterResource(if (block.autoSpeak) R.drawable.ms_volume_up else R.drawable.ms_volume_off),
                                contentDescription = null,
                                modifier = Modifier.size(14.dp),
                            )
                        }
                    }
                } else {
                    // Input node: eye + render mode + copy + speak (like Windows)
                    var showInputRenderMenu by remember { mutableStateOf(false) }
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(2.dp),
                    ) {
                        // Eye toggle
                        androidx.compose.material3.IconToggleButton(
                            checked = block.showOverlay,
                            onCheckedChange = { onBlockUpdated(block.copy(showOverlay = it)) },
                            modifier = Modifier.size(24.dp),
                        ) {
                            Icon(
                                painter = painterResource(if (block.showOverlay) R.drawable.ms_visibility else R.drawable.ms_visibility_off),
                                contentDescription = null,
                                modifier = Modifier.size(14.dp),
                            )
                        }

                        // Stream mode pill for input node
                        if (block.showOverlay) {
                            val isStreaming = block.streamingEnabled || block.renderMode == "markdown_stream"
                            val streamLabel = when (lang) {
                                "vi" -> if (isStreaming) "Stream" else "Không stream"
                                "ko" -> if (isStreaming) "스트림" else "스트림 없음"
                                else -> if (isStreaming) "Stream" else "No Stream"
                            }
                            Box {
                                Surface(
                                    shape = RoundedCornerShape(4.dp),
                                    color = pillBg,
                                    modifier = Modifier.height(20.dp)
                                        .pointerInput(Unit) { detectTapGestures { showInputRenderMenu = true } },
                                ) {
                                    Text(
                                        streamLabel,
                                        modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                        style = MaterialTheme.typography.labelSmall,
                                        fontSize = 9.sp,
                                        color = contentCol,
                                    )
                                }
                                androidx.compose.material3.DropdownMenu(
                                    expanded = showInputRenderMenu,
                                    onDismissRequest = { showInputRenderMenu = false },
                                ) {
                                    val streamOff = when (lang) { "vi" -> "Không stream"; "ko" -> "스트림 없음"; else -> "No Stream" }
                                    listOf(
                                        streamOff to false,
                                        "Stream" to true,
                                    ).forEach { (label, streaming) ->
                                        androidx.compose.material3.DropdownMenuItem(
                                            text = { Text(label, style = MaterialTheme.typography.bodySmall) },
                                            onClick = {
                                                val mode = if (streaming) "markdown_stream" else "markdown"
                                                onBlockUpdated(block.copy(renderMode = mode, streamingEnabled = streaming))
                                                showInputRenderMenu = false
                                            },
                                        )
                                    }
                                }
                            }
                        }

                        Spacer(Modifier.weight(1f))

                        // Copy toggle (distinct icons for on/off)
                        androidx.compose.material3.IconToggleButton(
                            checked = block.autoCopy,
                            onCheckedChange = { onBlockUpdated(block.copy(autoCopy = it)) },
                            modifier = Modifier.size(24.dp),
                        ) {
                            Icon(
                                imageVector = if (block.autoCopy) FileCopyIcon
                                    else FileCopyOffIcon,
                                contentDescription = null,
                                modifier = Modifier.size(14.dp),
                            )
                        }

                        // Speak toggle (distinct icons for on/off)
                        androidx.compose.material3.IconToggleButton(
                            checked = block.autoSpeak,
                            onCheckedChange = { onBlockUpdated(block.copy(autoSpeak = it)) },
                            modifier = Modifier.size(24.dp),
                        ) {
                            Icon(
                                painter = painterResource(if (block.autoSpeak) R.drawable.ms_volume_up else R.drawable.ms_volume_off),
                                contentDescription = null,
                                modifier = Modifier.size(14.dp),
                            )
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Main composable
// ---------------------------------------------------------------------------

@Composable
fun NodeGraphCanvas(
    state: NodeGraphState,
    onNodeMoved: (nodeId: String, x: Float, y: Float) -> Unit,
    onConnectionAdded: (fromId: String, toId: String) -> Unit,
    onConnectionRemoved: (fromId: String, toId: String) -> Unit,
    onNodeDeleted: (nodeId: String) -> Unit,
    onBlockUpdated: (nodeId: String, block: ProcessingBlock) -> Unit,
    onNodeTapped: (nodeId: String) -> Unit,
    onPromptEditRequest: (nodeId: String, currentPrompt: String) -> Unit = { _, _ -> },
    modifier: Modifier = Modifier,
    lang: String = "en",
    selectedNodeId: String? = null,
    presetType: dev.screengoated.toolbox.mobile.shared.preset.PresetType =
        dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT,
    providerSettings: dev.screengoated.toolbox.mobile.preset.PresetProviderSettings =
        dev.screengoated.toolbox.mobile.preset.PresetProviderSettings(),
) {
    val density = LocalDensity.current
    val nodeWidthPx = with(density) { NODE_WIDTH_DP.toPx() }
    val pinHitRadiusPx = with(density) { PIN_HIT_RADIUS_DP.toPx() }
    val gridSpacingPx = with(density) { GRID_SPACING_DP.toPx() }
    val bezierStrokePx = with(density) { 2.5f.dp.toPx() }
    val wireHitThreshold = with(density) { 16.dp.toPx() }

    val colorScheme = MaterialTheme.colorScheme
    val gridDotColor = remember(colorScheme) { colorScheme.onSurface.copy(alpha = 0.15f) }
    val bezierColor = remember(colorScheme) { colorScheme.primary.copy(alpha = 0.55f) }

    // Pan & zoom state
    var panOffset by remember { mutableStateOf(Offset.Zero) }
    var zoom by remember { mutableFloatStateOf(1f) }

    // Wire drag state
    var dragFromNodeId by remember { mutableStateOf<String?>(null) }
    var dragWireEnd by remember { mutableStateOf(Offset.Zero) }
    var dragWireCumulative by remember { mutableStateOf(Offset.Zero) }

    // Internal mutable positions — avoids stale closure in pointerInput
    val positions = remember { mutableStateMapOf<String, Offset>() }

    // Measured node heights (updated via onGloballyPositioned)
    val measuredHeights = remember { mutableStateMapOf<String, Float>() }

    // Canvas size for auto-centering
    var canvasSize by remember { mutableStateOf(Offset.Zero) }

    // Seed positions from BFS layout on first load / node list change
    val nodeIds = remember(state.nodes.map { it.id }) { state.nodes.map { it.id } }
    val needsBfsLayout = remember(nodeIds) {
        state.nodes.all { it.x == 0f && it.y == 0f } && state.nodes.isNotEmpty()
    }
    val bfsNodes = remember(nodeIds, state.connections, nodeWidthPx) {
        if (needsBfsLayout) {
            bfsLayout(state.nodes, state.connections, nodeWidthPx, DEFAULT_NODE_HEIGHT_PX)
        } else {
            state.nodes
        }
    }
    bfsNodes.forEach { node ->
        if (node.id !in positions) {
            if (node.x == 0f && node.y == 0f && positions.isNotEmpty()) {
                // New node with no layout position — place it to the right of existing nodes
                val maxX = positions.values.maxOfOrNull { it.x } ?: 0f
                val avgY = positions.values.map { it.y }.average().toFloat()
                val newX = maxX + nodeWidthPx * 1.6f
                android.util.Log.d("NodeGraph", "New node ${node.id} placed at ($newX, $avgY) — existing maxX=$maxX")
                positions[node.id] = Offset(newX, avgY)
            } else {
                android.util.Log.d("NodeGraph", "Seed node ${node.id} at (${node.x}, ${node.y})")
                positions[node.id] = Offset(node.x, node.y)
            }
        }
    }
    val currentIds = state.nodes.map { it.id }.toSet()
    positions.keys.removeAll { it !in currentIds }

    // Layout state flags
    var hasAutoFit by remember { mutableStateOf(false) }
    var hasResolvedCollisions by remember { mutableStateOf(false) }
    LaunchedEffect(nodeIds) {
        hasResolvedCollisions = false  // reset when nodes change
        hasAutoFit = false
    }
    if (!hasResolvedCollisions && measuredHeights.size >= state.nodes.size && state.nodes.size > 1) {
        // Group nodes by approximate X column (same BFS layer)
        val columnThreshold = nodeWidthPx * 0.5f
        val sortedByX = state.nodes.mapNotNull { node ->
            val pos = positions[node.id] ?: return@mapNotNull null
            Triple(node.id, pos, measuredHeights[node.id] ?: DEFAULT_NODE_HEIGHT_PX)
        }.sortedBy { it.second.x }

        // Group into columns
        val columns = mutableListOf<MutableList<Triple<String, Offset, Float>>>()
        for (entry in sortedByX) {
            val lastCol = columns.lastOrNull()
            if (lastCol != null && kotlin.math.abs(entry.second.x - lastCol.first().second.x) < columnThreshold) {
                lastCol.add(entry)
            } else {
                columns.add(mutableListOf(entry))
            }
        }

        // For each column, sort by Y and push apart if overlapping
        var anyFixed = false
        for (col in columns) {
            if (col.size < 2) continue
            val sorted = col.sortedBy { it.second.y }
            for (i in 1 until sorted.size) {
                val prevId = sorted[i - 1].first
                val prevPos = positions[prevId] ?: continue
                val prevHeight = measuredHeights[prevId] ?: DEFAULT_NODE_HEIGHT_PX
                val curId = sorted[i].first
                val curPos = positions[curId] ?: continue
                val gap = 30f // minimum gap in px between nodes
                val minY = prevPos.y + prevHeight + gap
                if (curPos.y < minY) {
                    positions[curId] = Offset(curPos.x, minY)
                    anyFixed = true
                }
            }
        }
        hasResolvedCollisions = true
    }

    // Auto zoom-to-fit and center on first layout
    LaunchedEffect(nodeIds, canvasSize, hasResolvedCollisions) {
        if (canvasSize == Offset.Zero || positions.isEmpty() || hasAutoFit) return@LaunchedEffect
        if (!hasResolvedCollisions && state.nodes.size > 1) return@LaunchedEffect

        val allEntries = state.nodes.mapNotNull { node ->
            val pos = positions[node.id] ?: return@mapNotNull null
            val h = measuredHeights[node.id] ?: DEFAULT_NODE_HEIGHT_PX
            Triple(pos, h, node.id)
        }
        if (allEntries.isEmpty()) return@LaunchedEffect
        val minX = allEntries.minOf { it.first.x }
        val minY = allEntries.minOf { it.first.y }
        val maxX = allEntries.maxOf { it.first.x } + nodeWidthPx
        val maxY = allEntries.maxOf { it.first.y + it.second }

        val contentW = (maxX - minX).coerceAtLeast(1f)
        val contentH = (maxY - minY).coerceAtLeast(1f)

        // Compute zoom to fit content in canvas with padding
        val availW = canvasSize.x - FIT_PADDING * 2f
        val availH = canvasSize.y - FIT_PADDING * 2f
        val fitZoom = minOf(availW / contentW, availH / contentH)
            .coerceIn(MIN_ZOOM, 1f) // don't zoom in beyond 1x for auto-fit

        zoom = fitZoom
        // Center the content
        panOffset = Offset(
            (canvasSize.x - contentW * fitZoom) / 2f - minX * fitZoom,
            (canvasSize.y - contentH * fitZoom) / 2f - minY * fitZoom,
        )
        hasAutoFit = true
    }

    // Build display list from current state + mutable positions
    val layoutNodes = state.nodes.map { node ->
        val pos = positions[node.id] ?: Offset(node.x, node.y)
        node.copy(x = pos.x, y = pos.y)
    }
    val nodeMap = layoutNodes.associateBy { it.id }

    Box(
        modifier = modifier
            .fillMaxSize()
            .clipToBounds()
            .background(MaterialTheme.colorScheme.surfaceContainerLowest)
            .onGloballyPositioned { coords ->
                canvasSize = Offset(coords.size.width.toFloat(), coords.size.height.toFloat())
            }
            // Focal-point pinch zoom + two-finger pan
            .pointerInput(Unit) {
                detectTransformGestures { centroid, pan, zoomChange, _ ->
                    val oldZoom = zoom
                    val newZoom = (oldZoom * zoomChange).coerceIn(MIN_ZOOM, MAX_ZOOM)
                    // Zoom around the pinch centroid: keep the content point
                    // under the centroid fixed in screen space
                    val zoomDelta = newZoom / oldZoom
                    panOffset = centroid - (centroid - panOffset) * zoomDelta + pan
                    zoom = newZoom
                }
            }
            // Tap on empty canvas: deselect + check wire taps
            .pointerInput(state.connections, layoutNodes, zoom, panOffset) {
                detectTapGestures { tapOffset ->
                    val canvasX = (tapOffset.x - panOffset.x) / zoom
                    val canvasY = (tapOffset.y - panOffset.y) / zoom

                    for (conn in state.connections) {
                        val fromNode = nodeMap[conn.fromNodeId] ?: continue
                        val toNode = nodeMap[conn.toNodeId] ?: continue
                        val from = outputPinCenter(fromNode, nodeWidthPx, measuredHeights)
                        val to = inputPinCenter(toNode, measuredHeights)
                        if (isNearBezier(Offset(canvasX, canvasY), from, to, wireHitThreshold / zoom)) {
                            onConnectionRemoved(conn.fromNodeId, conn.toNodeId)
                            return@detectTapGestures
                        }
                    }
                    onNodeTapped("")
                }
            },
    ) {
        // Layer 1: grid dots + bezier connections + drag wire preview
        Canvas(modifier = Modifier.fillMaxSize()) {
            drawGridDots(gridSpacingPx, gridDotColor, panOffset, zoom)

            for (conn in state.connections) {
                val fromNode = nodeMap[conn.fromNodeId] ?: continue
                val toNode = nodeMap[conn.toNodeId] ?: continue
                val from = outputPinCenter(fromNode, nodeWidthPx, measuredHeights)
                val to = inputPinCenter(toNode, measuredHeights)
                drawBezierConnection(
                    from = Offset(from.x * zoom + panOffset.x, from.y * zoom + panOffset.y),
                    to = Offset(to.x * zoom + panOffset.x, to.y * zoom + panOffset.y),
                    color = bezierColor,
                    strokeWidthPx = bezierStrokePx * zoom,
                )
            }

            val dragFrom = dragFromNodeId
            if (dragFrom != null) {
                val fromNode = nodeMap[dragFrom]
                if (fromNode != null) {
                    val from = outputPinCenter(fromNode, nodeWidthPx, measuredHeights)
                    val screenFrom = Offset(from.x * zoom + panOffset.x, from.y * zoom + panOffset.y)
                    drawBezierConnection(
                        from = screenFrom,
                        to = dragWireEnd,
                        color = WIRE_DRAG_COLOR,
                        strokeWidthPx = bezierStrokePx * zoom * 1.2f,
                        alpha = 0.8f,
                    )
                }
            }
        }

        // Layer 2: node cards
        for (node in layoutNodes) {
            key(node.id) {
                val screenX = node.x * zoom + panOffset.x
                val screenY = node.y * zoom + panOffset.y

                NodeCard(
                    node = node,
                    isSelected = selectedNodeId == node.id,
                    onTap = { onNodeTapped(node.id) },
                    presetType = presetType,
                    onDrag = { dx, dy ->
                        // graphicsLayer already scales pointer coords, so dx/dy are in canvas space
                        val cur = positions[node.id] ?: Offset(node.x, node.y)
                        val newPos = Offset(cur.x + dx, cur.y + dy)
                        positions[node.id] = newPos
                    },
                    onDragEnd = {
                        // Commit final position to preset
                        val pos = positions[node.id]
                        if (pos != null) {
                            android.util.Log.d("NodeGraph", "DragEnd node=${node.id} → pos=(${pos.x}, ${pos.y})")
                            onNodeMoved(node.id, pos.x, pos.y)
                        }
                    },
                    onDelete = {
                        if (node.block.blockType != BlockType.INPUT_ADAPTER) {
                            onNodeDeleted(node.id)
                        }
                    },
                    onOutputPinDragStart = {
                        dragFromNodeId = node.id
                        val pinPos = outputPinCenter(node, nodeWidthPx, measuredHeights)
                        dragWireEnd = Offset(pinPos.x * zoom + panOffset.x, pinPos.y * zoom + panOffset.y)
                        dragWireCumulative = Offset.Zero
                    },
                    onOutputPinDrag = { delta ->
                        // delta is in card-local (canvas) space due to graphicsLayer; convert to screen space
                        val screenDelta = Offset(delta.x * zoom, delta.y * zoom)
                        dragWireCumulative += screenDelta
                        val fromNode = nodeMap[node.id] ?: return@NodeCard
                        val pinPos = outputPinCenter(fromNode, nodeWidthPx, measuredHeights)
                        val screenPin = Offset(pinPos.x * zoom + panOffset.x, pinPos.y * zoom + panOffset.y)
                        dragWireEnd = screenPin + dragWireCumulative
                    },
                    onOutputPinDragEnd = {
                        val fromId = dragFromNodeId
                        if (fromId != null) {
                            val dropCanvas = Offset(
                                (dragWireEnd.x - panOffset.x) / zoom,
                                (dragWireEnd.y - panOffset.y) / zoom,
                            )
                            val target = layoutNodes.firstOrNull { candidate ->
                                if (candidate.id == fromId) return@firstOrNull false
                                if (candidate.block.blockType == BlockType.INPUT_ADAPTER) return@firstOrNull false
                                val pin = inputPinCenter(candidate, measuredHeights)
                                val dist = kotlin.math.sqrt(
                                    (dropCanvas.x - pin.x) * (dropCanvas.x - pin.x) +
                                        (dropCanvas.y - pin.y) * (dropCanvas.y - pin.y),
                                )
                                dist < pinHitRadiusPx / zoom * 3f
                            }
                            if (target != null) {
                                val fromNode = nodeMap[fromId]
                                if (fromNode != null && canConnect(fromNode, target, state.connections)) {
                                    onConnectionAdded(fromId, target.id)
                                }
                            }
                        }
                        dragFromNodeId = null
                        dragWireCumulative = Offset.Zero
                    },
                    onBlockUpdated = { updatedBlock -> onBlockUpdated(node.id, updatedBlock) },
                    onPromptEditRequest = {
                        onPromptEditRequest(node.id, node.block.prompt)
                    },
                    providerSettings = providerSettings,
                    onMeasured = { heightPx ->
                        measuredHeights[node.id] = heightPx
                    },
                    modifier = Modifier
                        .offset { IntOffset(screenX.roundToInt(), screenY.roundToInt()) }
                        .graphicsLayer {
                            scaleX = zoom
                            scaleY = zoom
                            transformOrigin = TransformOrigin(0f, 0f)
                        },
                    lang = lang,
                )
            }
        }
    }
}

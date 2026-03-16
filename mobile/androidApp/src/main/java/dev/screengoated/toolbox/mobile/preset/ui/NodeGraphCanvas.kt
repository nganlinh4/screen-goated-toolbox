@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
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

private val NODE_WIDTH_DP = 140.dp
private val HEADER_HEIGHT_DP = 5.dp
private val PIN_RADIUS_DP = 7.dp
private val PIN_HIT_RADIUS_DP = 20.dp
private val GRID_SPACING_DP = 24.dp
private const val GRID_DOT_RADIUS = 1.5f
private const val MIN_ZOOM = 0.25f
private const val MAX_ZOOM = 2.5f
private const val DEFAULT_NODE_HEIGHT_PX = 280f // fallback before measurement
private const val FIT_PADDING = 40f // px padding around auto-fit content

// Layout gap between nodes (fraction of node dimension)
private const val LAYOUT_GAP_X_RATIO = 0.6f  // horizontal gap = 60% of node width
private const val LAYOUT_GAP_Y_RATIO = 0.25f // vertical gap = 25% of node height

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

private fun headerColor(blockType: BlockType): Color = when (blockType) {
    BlockType.INPUT_ADAPTER -> Color(0xFF26A69A)
    BlockType.IMAGE -> Color(0xFFFFA726)
    BlockType.TEXT -> Color(0xFF42A5F5)
    BlockType.AUDIO -> Color(0xFFAB47BC)
}

private val PIN_INPUT_COLOR = Color(0xFF66BB6A)
private val PIN_OUTPUT_COLOR = Color(0xFF42A5F5)
private val WIRE_DRAG_COLOR = Color(0xFFFFAB40)

// ---------------------------------------------------------------------------
// BFS layout for nodes with no stored position
// ---------------------------------------------------------------------------

/**
 * BFS-based layer layout matching the Windows `blocks_to_snarl` algorithm.
 * Each layer is vertically centered around [LAYOUT_CENTER_Y].
 */
internal fun bfsLayout(
    nodes: List<NodePosition>,
    connections: List<Connection>,
): List<NodePosition> {
    if (nodes.isEmpty()) return nodes

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
    val posMap = mutableMapOf<String, Offset>()

    nodes.forEach { node ->
        val depth = depthOf[node.id] ?: 0
        layerNodes.getOrPut(depth) { mutableListOf() }.add(node)
    }

    // Assign positions — each layer vertically centered around LAYOUT_CENTER_Y
    for ((depth, nodesInLayer) in layerNodes) {
        val count = nodesInLayer.size
        val layerHeight = count.toFloat() * LAYOUT_SPACING_Y
        val layerStartY = LAYOUT_CENTER_Y - (layerHeight / 2f) + (LAYOUT_SPACING_Y / 2f)

        nodesInLayer.forEachIndexed { i, node ->
            val x = LAYOUT_START_X + depth.toFloat() * LAYOUT_SPACING_X
            val y = layerStartY + i.toFloat() * LAYOUT_SPACING_Y
            posMap[node.id] = Offset(x, y)
        }
    }

    // Fallback for unreachable nodes
    nodes.filter { it.id !in visited }.forEachIndexed { i, node ->
        posMap[node.id] = Offset(
            LAYOUT_START_X + i * LAYOUT_SPACING_X,
            LAYOUT_CENTER_Y + 300f,
        )
    }

    return nodes.map { node ->
        val pos = posMap[node.id] ?: Offset(node.x, node.y)
        node.copy(x = pos.x, y = pos.y)
    }
}

// ---------------------------------------------------------------------------
// Pin position helpers — use actual measured height per node
// ---------------------------------------------------------------------------

private fun inputPinCenter(
    node: NodePosition,
    measuredHeights: Map<String, Float>,
): Offset {
    val h = measuredHeights[node.id] ?: DEFAULT_NODE_HEIGHT_PX
    return Offset(node.x, node.y + h / 2f)
}

private fun outputPinCenter(
    node: NodePosition,
    nodeWidthPx: Float,
    measuredHeights: Map<String, Float>,
): Offset {
    val h = measuredHeights[node.id] ?: DEFAULT_NODE_HEIGHT_PX
    return Offset(node.x + nodeWidthPx, node.y + h / 2f)
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

private fun nodeTypeLabel(blockType: BlockType, lang: String): String = when (blockType) {
    BlockType.INPUT_ADAPTER -> when (lang) { "vi" -> "Đầu vào"; "ko" -> "입력"; else -> "Input" }
    BlockType.IMAGE -> when (lang) { "vi" -> "Đặc biệt"; "ko" -> "특수"; else -> "Special" }
    BlockType.TEXT -> when (lang) { "vi" -> "Xử lý"; "ko" -> "처리"; else -> "Process" }
    BlockType.AUDIO -> when (lang) { "vi" -> "Đặc biệt"; "ko" -> "특수"; else -> "Special" }
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
    onLongPress: () -> Unit,
    onOutputPinDragStart: () -> Unit,
    onOutputPinDrag: (Offset) -> Unit,
    onOutputPinDragEnd: () -> Unit,
    onMeasured: (heightPx: Float) -> Unit,
    modifier: Modifier = Modifier,
    lang: String = "en",
) {
    val block = node.block
    val accentColor = headerColor(block.blockType)

    Card(
        modifier = modifier
            .width(NODE_WIDTH_DP)
            .onGloballyPositioned { coords -> onMeasured(coords.size.height.toFloat()) }
            .pointerInput(node.id) {
                detectTapGestures(
                    onTap = { onTap() },
                    onLongPress = { onLongPress() },
                )
            }
            .pointerInput(node.id + "_drag") {
                detectDragGestures { change, dragAmount ->
                    change.consume()
                    onDrag(dragAmount.x, dragAmount.y)
                }
            },
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
        ),
        border = if (isSelected) {
            androidx.compose.foundation.BorderStroke(2.dp, MaterialTheme.colorScheme.primary)
        } else {
            null
        },
        shape = MaterialTheme.shapes.medium,
    ) {
        Column {
            // Colored header bar
            Surface(
                modifier = Modifier.fillMaxWidth().height(HEADER_HEIGHT_DP),
                color = accentColor,
                content = {},
            )

            Row(
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
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

                // Content
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = nodeTypeLabel(block.blockType, lang),
                        style = MaterialTheme.typography.labelMedium,
                        fontWeight = FontWeight.Bold,
                        color = accentColor,
                    )
                    if (block.model.isNotBlank()) {
                        Text(
                            text = ModelCatalog.displayName(block.model),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    if (block.prompt.isNotBlank()) {
                        Spacer(Modifier.height(2.dp))
                        Text(
                            text = block.prompt,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 3,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }

                // Output pin (draggable to create connections)
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
    modifier: Modifier = Modifier,
    lang: String = "en",
    selectedNodeId: String? = null,
) {
    val density = LocalDensity.current
    val nodeWidthPx = with(density) { NODE_WIDTH_DP.toPx() }
    val pinHitRadiusPx = with(density) { PIN_HIT_RADIUS_DP.toPx() }
    val gridSpacingPx = with(density) { GRID_SPACING_DP.toPx() }
    val bezierStrokePx = with(density) { 2.5f.dp.toPx() }
    val wireHitThreshold = with(density) { 16.dp.toPx() }

    val colorScheme = MaterialTheme.colorScheme
    val gridDotColor = remember(colorScheme) { colorScheme.onSurface.copy(alpha = 0.06f) }
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
    val bfsNodes = remember(nodeIds, state.connections) {
        if (needsBfsLayout) bfsLayout(state.nodes, state.connections) else state.nodes
    }
    bfsNodes.forEach { node ->
        if (node.id !in positions) {
            positions[node.id] = Offset(node.x, node.y)
        }
    }
    val currentIds = state.nodes.map { it.id }.toSet()
    positions.keys.removeAll { it !in currentIds }

    // Auto zoom-to-fit and center on first layout
    var hasAutoFit by remember { mutableStateOf(false) }
    LaunchedEffect(nodeIds, canvasSize) {
        if (canvasSize == Offset.Zero || positions.isEmpty() || hasAutoFit) return@LaunchedEffect

        val allPos = positions.values.toList()
        val minX = allPos.minOf { it.x }
        val minY = allPos.minOf { it.y }
        val maxX = allPos.maxOf { it.x } + nodeWidthPx
        val maxY = allPos.maxOf { it.y } + DEFAULT_NODE_HEIGHT_PX

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
                    onDrag = { dx, dy ->
                        // graphicsLayer already transforms pointer coords by 1/zoom,
                        // so dx/dy are already in canvas space — do NOT divide by zoom
                        val cur = positions[node.id] ?: Offset(node.x, node.y)
                        val newPos = Offset(cur.x + dx, cur.y + dy)
                        positions[node.id] = newPos
                        onNodeMoved(node.id, newPos.x, newPos.y)
                    },
                    onLongPress = {
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
                        dragWireCumulative += delta
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
                                dist < pinHitRadiusPx / zoom * 2f
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

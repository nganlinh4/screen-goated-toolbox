@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
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
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
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

private val NODE_WIDTH_DP = 200.dp
private val NODE_HEIGHT_ESTIMATE_DP = 88.dp
private val HEADER_HEIGHT_DP = 5.dp
private val PIN_RADIUS_DP = 7.dp
private val PIN_HIT_RADIUS_DP = 20.dp
private val GRID_SPACING_DP = 24.dp
private const val GRID_DOT_RADIUS = 1.5f
private const val MIN_ZOOM = 0.4f
private const val MAX_ZOOM = 2.0f

private val LAYER_X_OFFSETS = floatArrayOf(20f, 260f, 500f, 740f, 980f)
private const val VERTICAL_SPACING = 160f

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

private fun headerColor(blockType: BlockType): Color = when (blockType) {
    BlockType.INPUT_ADAPTER -> Color(0xFF26A69A) // teal
    BlockType.IMAGE -> Color(0xFFFFA726)          // amber
    BlockType.TEXT -> Color(0xFF42A5F5)            // blue
    BlockType.AUDIO -> Color(0xFFAB47BC)           // purple
}

private val PIN_INPUT_COLOR = Color(0xFF66BB6A)  // green
private val PIN_OUTPUT_COLOR = Color(0xFF42A5F5) // blue
private val WIRE_DRAG_COLOR = Color(0xFFFFAB40)  // orange accent for active drag

// ---------------------------------------------------------------------------
// BFS layout for nodes with no stored position
// ---------------------------------------------------------------------------

internal fun bfsLayout(
    nodes: List<NodePosition>,
    connections: List<Connection>,
): List<NodePosition> {
    if (nodes.isEmpty()) return nodes

    val adjacency = mutableMapOf<String, MutableList<String>>()
    nodes.forEach { adjacency[it.id] = mutableListOf() }
    connections.forEach { c ->
        adjacency[c.fromNodeId]?.add(c.toNodeId)
    }

    val hasIncoming = connections.map { it.toNodeId }.toSet()
    val roots = nodes.filter { it.id !in hasIncoming }.map { it.id }
        .ifEmpty { listOf(nodes.first().id) }

    val layerOf = mutableMapOf<String, Int>()
    val queue = ArrayDeque<String>()
    roots.forEach { r ->
        layerOf[r] = 0
        queue.add(r)
    }
    while (queue.isNotEmpty()) {
        val cur = queue.removeFirst()
        val nextLayer = (layerOf[cur] ?: 0) + 1
        adjacency[cur]?.forEach { neighbor ->
            if (neighbor !in layerOf) {
                layerOf[neighbor] = nextLayer
                queue.add(neighbor)
            }
        }
    }
    nodes.forEach { if (it.id !in layerOf) layerOf[it.id] = 0 }

    val layers = nodes.groupBy { layerOf[it.id] ?: 0 }
    return layers.flatMap { (layer, layerNodes) ->
        val xBase = LAYER_X_OFFSETS.getOrElse(layer) {
            LAYER_X_OFFSETS.last() + (layer - LAYER_X_OFFSETS.lastIndex) * 240f
        }
        layerNodes.mapIndexed { idx, node ->
            node.copy(x = xBase, y = 20f + idx * VERTICAL_SPACING)
        }
    }
}

// ---------------------------------------------------------------------------
// Pin position helpers (in px, relative to canvas origin)
// ---------------------------------------------------------------------------

private fun inputPinCenter(node: NodePosition, nodeHeightPx: Float): Offset {
    return Offset(node.x, node.y + nodeHeightPx / 2f)
}

private fun outputPinCenter(node: NodePosition, nodeWidthPx: Float, nodeHeightPx: Float): Offset {
    return Offset(node.x + nodeWidthPx, node.y + nodeHeightPx / 2f)
}

// ---------------------------------------------------------------------------
// Connection validation (matches Windows viewer.rs rules)
// ---------------------------------------------------------------------------

internal fun canConnect(
    fromNode: NodePosition,
    toNode: NodePosition,
    existingConnections: List<Connection>,
): Boolean {
    // No self-loops
    if (fromNode.id == toNode.id) return false

    // Target must not be INPUT_ADAPTER (input nodes have no input pin)
    if (toNode.block.blockType == BlockType.INPUT_ADAPTER) return false

    // Single input per node: target already has an incoming connection
    if (existingConnections.any { it.toNodeId == toNode.id }) return false

    // Special nodes (IMAGE/AUDIO used as "special" first-level processor)
    // can only receive input from INPUT_ADAPTER
    val isSpecialTarget = toNode.block.blockType == BlockType.IMAGE ||
        toNode.block.blockType == BlockType.AUDIO
    if (isSpecialTarget && fromNode.block.blockType != BlockType.INPUT_ADAPTER) return false

    // No duplicate connections
    if (existingConnections.any { it.fromNodeId == fromNode.id && it.toNodeId == toNode.id }) return false

    return true
}

// ---------------------------------------------------------------------------
// Canvas drawing helpers
// ---------------------------------------------------------------------------

private fun DrawScope.drawGridDots(gridSpacingPx: Float, dotColor: Color, pan: Offset, zoom: Float) {
    val w = size.width
    val h = size.height
    val step = gridSpacingPx * zoom
    if (step < 4f) return // too dense, skip

    val offsetX = pan.x % step
    val offsetY = pan.y % step
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
    from: Offset,
    to: Offset,
    color: Color,
    strokeWidthPx: Float,
    alpha: Float = 1f,
) {
    val dx = (to.x - from.x).coerceAtLeast(40f) * 0.45f
    val path = Path().apply {
        moveTo(from.x, from.y)
        cubicTo(from.x + dx, from.y, to.x - dx, to.y, to.x, to.y)
    }
    drawPath(path, color.copy(alpha = alpha), style = Stroke(width = strokeWidthPx, cap = StrokeCap.Round))
}

/** Check if a point is close to a bezier wire (for tap-to-delete). */
private fun isNearBezier(
    point: Offset,
    from: Offset,
    to: Offset,
    threshold: Float,
): Boolean {
    val dx = (to.x - from.x).coerceAtLeast(40f) * 0.45f
    // Sample 20 points along the curve
    for (i in 0..20) {
        val t = i / 20f
        val mt = 1f - t
        val x = mt * mt * mt * from.x +
            3f * mt * mt * t * (from.x + dx) +
            3f * mt * t * t * (to.x - dx) +
            t * t * t * to.x
        val y = mt * mt * mt * from.y +
            3f * mt * mt * t * from.y +
            3f * mt * t * t * to.y +
            t * t * t * to.y
        val dist = kotlin.math.sqrt((point.x - x) * (point.x - x) + (point.y - y) * (point.y - y))
        if (dist < threshold) return true
    }
    return false
}

// ---------------------------------------------------------------------------
// Localized node type label
// ---------------------------------------------------------------------------

private fun nodeTypeLabel(blockType: BlockType, lang: String): String = when (blockType) {
    BlockType.INPUT_ADAPTER -> when (lang) {
        "vi" -> "Đầu vào"
        "ko" -> "입력"
        else -> "Input"
    }
    BlockType.IMAGE -> when (lang) {
        "vi" -> "Đặc biệt"
        "ko" -> "특수"
        else -> "Special"
    }
    BlockType.TEXT -> when (lang) {
        "vi" -> "Xử lý"
        "ko" -> "처리"
        else -> "Process"
    }
    BlockType.AUDIO -> when (lang) {
        "vi" -> "Đặc biệt"
        "ko" -> "특수"
        else -> "Special"
    }
}

// ---------------------------------------------------------------------------
// Node card composable (M3 Expressive)
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
    modifier: Modifier = Modifier,
    lang: String = "en",
) {
    val block = node.block
    val accentColor = headerColor(block.blockType)
    val borderColor = if (isSelected) {
        MaterialTheme.colorScheme.primary
    } else {
        Color.Transparent
    }

    Card(
        modifier = modifier
            .width(NODE_WIDTH_DP)
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
            androidx.compose.foundation.BorderStroke(2.dp, borderColor)
        } else {
            null
        },
        shape = MaterialTheme.shapes.medium,
    ) {
        Column {
            // Colored header bar
            Surface(
                modifier = Modifier.width(NODE_WIDTH_DP).height(HEADER_HEIGHT_DP),
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
                            text = block.prompt.take(40) + if (block.prompt.length > 40) "..." else "",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
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
    val nodeHeightPx = with(density) { NODE_HEIGHT_ESTIMATE_DP.toPx() }
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

    // Internal mutable positions — avoids stale closure issue in pointerInput
    val positions = remember { mutableStateMapOf<String, Offset>() }

    // Seed positions from BFS layout or state on first load / node list change
    val nodeIds = remember(state.nodes.map { it.id }) { state.nodes.map { it.id } }
    val needsBfsLayout = remember(nodeIds) {
        state.nodes.all { it.x == 0f && it.y == 0f } && state.nodes.isNotEmpty()
    }
    val bfsNodes = remember(nodeIds, state.connections) {
        if (needsBfsLayout) bfsLayout(state.nodes, state.connections) else state.nodes
    }
    // Sync BFS results into mutable positions (only for new nodes)
    bfsNodes.forEach { node ->
        if (node.id !in positions) {
            positions[node.id] = Offset(node.x, node.y)
        }
    }
    // Remove stale entries
    val currentIds = state.nodes.map { it.id }.toSet()
    positions.keys.removeAll { it !in currentIds }

    // Build display list from current state + mutable positions
    val layoutNodes = state.nodes.map { node ->
        val pos = positions[node.id] ?: Offset(node.x, node.y)
        node.copy(x = pos.x, y = pos.y)
    }

    val nodeMap = layoutNodes.associateBy { it.id }

    // Pan/zoom gesture
    val transformState = rememberTransformableState { zoomChange, panChange, _ ->
        zoom = (zoom * zoomChange).coerceIn(MIN_ZOOM, MAX_ZOOM)
        panOffset += panChange
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .clipToBounds()
            .background(MaterialTheme.colorScheme.surfaceContainerLowest)
            .transformable(state = transformState)
            // Tap on empty canvas: deselect + check wire taps
            .pointerInput(state.connections, layoutNodes, zoom, panOffset) {
                detectTapGestures { tapOffset ->
                    // Convert tap to canvas coordinates
                    val canvasX = (tapOffset.x - panOffset.x) / zoom
                    val canvasY = (tapOffset.y - panOffset.y) / zoom

                    // Check if tap is near any wire
                    for (conn in state.connections) {
                        val fromNode = nodeMap[conn.fromNodeId] ?: continue
                        val toNode = nodeMap[conn.toNodeId] ?: continue
                        val from = outputPinCenter(fromNode, nodeWidthPx, nodeHeightPx)
                        val to = inputPinCenter(toNode, nodeHeightPx)
                        if (isNearBezier(Offset(canvasX, canvasY), from, to, wireHitThreshold / zoom)) {
                            onConnectionRemoved(conn.fromNodeId, conn.toNodeId)
                            return@detectTapGestures
                        }
                    }
                    // Tap on empty space — deselect
                    onNodeTapped("")
                }
            },
    ) {
        // Layer 1: grid dots + bezier connections + drag wire preview
        Canvas(modifier = Modifier.fillMaxSize()) {
            drawGridDots(gridSpacingPx, gridDotColor, panOffset, zoom)

            // Existing connections
            for (conn in state.connections) {
                val fromNode = nodeMap[conn.fromNodeId] ?: continue
                val toNode = nodeMap[conn.toNodeId] ?: continue
                val from = outputPinCenter(fromNode, nodeWidthPx, nodeHeightPx)
                val to = inputPinCenter(toNode, nodeHeightPx)
                drawBezierConnection(
                    from = Offset(from.x * zoom + panOffset.x, from.y * zoom + panOffset.y),
                    to = Offset(to.x * zoom + panOffset.x, to.y * zoom + panOffset.y),
                    color = bezierColor,
                    strokeWidthPx = bezierStrokePx * zoom,
                )
            }

            // Active drag wire preview
            val dragFrom = dragFromNodeId
            if (dragFrom != null) {
                val fromNode = nodeMap[dragFrom]
                if (fromNode != null) {
                    val from = outputPinCenter(fromNode, nodeWidthPx, nodeHeightPx)
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
                        val cur = positions[node.id] ?: Offset(node.x, node.y)
                        val newPos = Offset(cur.x + dx / zoom, cur.y + dy / zoom)
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
                        val pinPos = outputPinCenter(node, nodeWidthPx, nodeHeightPx)
                        dragWireEnd = Offset(pinPos.x * zoom + panOffset.x, pinPos.y * zoom + panOffset.y)
                        dragWireCumulative = Offset.Zero
                    },
                    onOutputPinDrag = { delta ->
                        dragWireCumulative += delta
                        val fromNode = nodeMap[node.id] ?: return@NodeCard
                        val pinPos = outputPinCenter(fromNode, nodeWidthPx, nodeHeightPx)
                        val screenPin = Offset(pinPos.x * zoom + panOffset.x, pinPos.y * zoom + panOffset.y)
                        dragWireEnd = screenPin + dragWireCumulative
                    },
                    onOutputPinDragEnd = {
                        val fromId = dragFromNodeId
                        if (fromId != null) {
                            // Find target node whose input pin is near the drop point
                            val dropCanvas = Offset(
                                (dragWireEnd.x - panOffset.x) / zoom,
                                (dragWireEnd.y - panOffset.y) / zoom,
                            )
                            val target = layoutNodes.firstOrNull { candidate ->
                                if (candidate.id == fromId) return@firstOrNull false
                                if (candidate.block.blockType == BlockType.INPUT_ADAPTER) return@firstOrNull false
                                val pin = inputPinCenter(candidate, nodeHeightPx)
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
                    modifier = Modifier
                        .offset { IntOffset(screenX.roundToInt(), screenY.roundToInt()) }
                        .graphicsLayer {
                            scaleX = zoom
                            scaleY = zoom
                            transformOrigin = androidx.compose.ui.graphics.TransformOrigin(0f, 0f)
                        },
                    lang = lang,
                )
            }
        }
    }
}

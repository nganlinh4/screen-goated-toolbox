@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.foundation.Canvas
import androidx.compose.runtime.key
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
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
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
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

private val NODE_WIDTH_DP = 220.dp
private val HEADER_HEIGHT_DP = 6.dp
private val PIN_SIZE_DP = 12.dp
private val GRID_SPACING_DP = 8.dp
private const val GRID_DOT_RADIUS = 1.5f

private val LAYER_X_OFFSETS = floatArrayOf(20f, 260f, 500f, 740f, 980f)
private const val VERTICAL_SPACING = 180f

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

    // Find root nodes (no incoming edges)
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
    // Assign any unreachable nodes to layer 0
    nodes.forEach { if (it.id !in layerOf) layerOf[it.id] = 0 }

    val layers = nodes.groupBy { layerOf[it.id] ?: 0 }
    return layers.flatMap { (layer, layerNodes) ->
        val xBase = LAYER_X_OFFSETS.getOrElse(layer) { LAYER_X_OFFSETS.last() + (layer - LAYER_X_OFFSETS.lastIndex) * 240f }
        layerNodes.mapIndexed { idx, node ->
            node.copy(
                x = xBase,
                y = 20f + idx * VERTICAL_SPACING,
            )
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
// Canvas: grid + bezier connections
// ---------------------------------------------------------------------------

private fun DrawScope.drawGridDots(
    gridSpacingPx: Float,
    dotColor: Color,
) {
    val w = size.width
    val h = size.height
    var x = 0f
    while (x < w) {
        var y = 0f
        while (y < h) {
            drawCircle(dotColor, radius = GRID_DOT_RADIUS, center = Offset(x, y))
            y += gridSpacingPx
        }
        x += gridSpacingPx
    }
}

private fun DrawScope.drawBezierConnection(
    from: Offset,
    to: Offset,
    color: Color,
    strokeWidthPx: Float,
) {
    val dx = (to.x - from.x) * 0.4f
    val path = Path().apply {
        moveTo(from.x, from.y)
        cubicTo(
            from.x + dx, from.y,
            to.x - dx, to.y,
            to.x, to.y,
        )
    }
    drawPath(path, color, style = Stroke(width = strokeWidthPx))
}

// ---------------------------------------------------------------------------
// Node card composable
// ---------------------------------------------------------------------------

@Composable
private fun NodeCard(
    node: NodePosition,
    onDrag: (dx: Float, dy: Float) -> Unit,
    onLongPress: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val block = node.block
    val accentColor = headerColor(block.blockType)

    Card(
        modifier = modifier
            .width(NODE_WIDTH_DP)
            .pointerInput(node.id) {
                detectDragGestures { change, dragAmount ->
                    change.consume()
                    onDrag(dragAmount.x, dragAmount.y)
                }
            }
            .pointerInput(node.id) {
                detectTapGestures(onLongPress = { onLongPress() })
            },
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
        ),
        shape = MaterialTheme.shapes.extraSmall,
    ) {
        Column {
            // Colored header bar
            Surface(
                modifier = Modifier
                    .width(NODE_WIDTH_DP)
                    .height(HEADER_HEIGHT_DP),
                color = accentColor,
                content = {},
            )

            Row(
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                // Input pin
                if (block.blockType != BlockType.INPUT_ADAPTER) {
                    Surface(
                        modifier = Modifier.size(PIN_SIZE_DP),
                        shape = CircleShape,
                        color = PIN_INPUT_COLOR,
                        content = {},
                    )
                    Spacer(Modifier.width(6.dp))
                }

                // Content
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = block.blockType.name.replace("_", " "),
                        style = MaterialTheme.typography.labelMedium,
                        fontWeight = FontWeight.Bold,
                        color = accentColor,
                    )
                    if (block.model.isNotBlank()) {
                        Text(
                            text = block.model,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    if (block.prompt.isNotBlank()) {
                        Spacer(Modifier.height(2.dp))
                        Text(
                            text = block.prompt.take(50) + if (block.prompt.length > 50) "..." else "",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }

                // Output pin
                Spacer(Modifier.width(6.dp))
                Surface(
                    modifier = Modifier.size(PIN_SIZE_DP),
                    shape = CircleShape,
                    color = PIN_OUTPUT_COLOR,
                    content = {},
                )
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
    modifier: Modifier = Modifier,
    lang: String = "en",
) {
    val density = LocalDensity.current
    val nodeWidthPx = with(density) { NODE_WIDTH_DP.toPx() }
    // Estimated card height: header(6dp) + padding(8+8dp) + ~3 lines text ~ 80dp
    val estimatedNodeHeightPx = with(density) { 80.dp.toPx() }
    val gridSpacingPx = with(density) { GRID_SPACING_DP.toPx() }
    val bezierStrokePx = with(density) { 3.dp.toPx() }

    val colorScheme = MaterialTheme.colorScheme
    val gridDotColor = remember(colorScheme) { colorScheme.onSurface.copy(alpha = 0.05f) }
    val bezierColor = remember(colorScheme) { colorScheme.primary.copy(alpha = 0.6f) }

    // Apply BFS layout for nodes that sit at origin (no stored position)
    val layoutNodes = remember(state.nodes, state.connections) {
        val needsLayout = state.nodes.all { it.x == 0f && it.y == 0f }
        if (needsLayout && state.nodes.isNotEmpty()) {
            bfsLayout(state.nodes, state.connections)
        } else {
            state.nodes
        }
    }

    val nodeMap = remember(layoutNodes) { layoutNodes.associateBy { it.id } }

    Box(
        modifier = modifier
            .fillMaxSize()
            .clipToBounds(),
    ) {
        // Layer 1: grid dots + bezier connections
        Canvas(modifier = Modifier.fillMaxSize()) {
            drawGridDots(gridSpacingPx, gridDotColor)

            for (conn in state.connections) {
                val fromNode = nodeMap[conn.fromNodeId] ?: continue
                val toNode = nodeMap[conn.toNodeId] ?: continue
                val from = outputPinCenter(fromNode, nodeWidthPx, estimatedNodeHeightPx)
                val to = inputPinCenter(toNode, estimatedNodeHeightPx)
                drawBezierConnection(from, to, bezierColor, bezierStrokePx)
            }
        }

        // Layer 2: node cards — key() ensures recomposition with fresh position
        for (node in layoutNodes) {
            key(node.id, node.x, node.y) {
                NodeCard(
                    node = node,
                    onDrag = { dx, dy ->
                        onNodeMoved(node.id, node.x + dx, node.y + dy)
                    },
                    onLongPress = {
                        onNodeDeleted(node.id)
                    },
                    modifier = Modifier.offset(
                        x = with(density) { node.x.toDp() },
                        y = with(density) { node.y.toDp() },
                    ),
                )
            }
        }
    }
}

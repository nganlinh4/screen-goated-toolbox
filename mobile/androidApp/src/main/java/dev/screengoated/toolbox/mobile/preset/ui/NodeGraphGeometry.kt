package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import dev.screengoated.toolbox.mobile.shared.preset.BlockType

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

internal fun inputPinCenter(
    node: NodePosition,
    measuredHeights: Map<String, Float>,
): Offset {
    return Offset(node.x + 10f, node.y + PIN_Y_OFFSET_PX)
}

internal fun outputPinCenter(
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

internal fun DrawScope.drawGridDots(gridSpacingPx: Float, dotColor: Color, pan: Offset, zoom: Float) {
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

internal fun DrawScope.drawBezierConnection(
    from: Offset, to: Offset, color: Color, strokeWidthPx: Float, alpha: Float = 1f,
) {
    val dx = (to.x - from.x).coerceAtLeast(40f) * 0.45f
    val path = Path().apply {
        moveTo(from.x, from.y)
        cubicTo(from.x + dx, from.y, to.x - dx, to.y, to.x, to.y)
    }
    drawPath(path, color.copy(alpha = alpha), style = Stroke(width = strokeWidthPx, cap = StrokeCap.Round))
}

internal fun isNearBezier(point: Offset, from: Offset, to: Offset, threshold: Float): Boolean {
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

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
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock
import kotlin.math.roundToInt

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
    onPromptEditRequest: (nodeId: String, currentPrompt: String) -> Unit = { _, _ -> },
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

@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class, androidx.compose.ui.text.ExperimentalTextApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.annotation.DrawableRes
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.DEFAULT_IMAGE_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetType

internal fun graphToPreset(graphState: NodeGraphState, currentPreset: Preset): Preset {
    if (graphState.nodes.isEmpty()) {
        return currentPreset.copy(blocks = emptyList(), blockConnections = emptyList())
    }

    val adjacency = mutableMapOf<String, MutableList<String>>()
    graphState.nodes.forEach { adjacency[it.id] = mutableListOf() }
    graphState.connections.forEach { c -> adjacency[c.fromNodeId]?.add(c.toNodeId) }

    val hasIncoming = graphState.connections.map { it.toNodeId }.toSet()
    val roots = graphState.nodes.filter { it.id !in hasIncoming }.map { it.id }
        .ifEmpty { listOf(graphState.nodes.first().id) }

    val visited = mutableSetOf<String>()
    val ordered = mutableListOf<NodePosition>()
    val queue = ArrayDeque<String>()
    roots.forEach { queue.add(it); visited.add(it) }
    while (queue.isNotEmpty()) {
        val cur = queue.removeFirst()
        val node = graphState.nodes.find { it.id == cur } ?: continue
        ordered.add(node)
        adjacency[cur]?.forEach { neighbor ->
            if (neighbor !in visited) { visited.add(neighbor); queue.add(neighbor) }
        }
    }
    graphState.nodes.filter { it.id !in visited }.forEach { ordered.add(it) }

    val idToIdx = ordered.mapIndexed { idx, n -> n.id to idx }.toMap()
    val blocks = ordered.map { it.block }
    val connections = graphState.connections.mapNotNull { c ->
        val fromIdx = idToIdx[c.fromNodeId] ?: return@mapNotNull null
        val toIdx = idToIdx[c.toNodeId] ?: return@mapNotNull null
        fromIdx to toIdx
    }
    return currentPreset.copy(blocks = blocks, blockConnections = connections)
}

@Composable
internal fun NodeGraphSection(
    editState: Preset,
    lang: String,
    onUpdate: (Preset) -> Unit,
    resetCounter: Int = 0,
    providerSettings: dev.screengoated.toolbox.mobile.preset.PresetProviderSettings =
        dev.screengoated.toolbox.mobile.preset.PresetProviderSettings(),
) {
    var graphState by remember(editState.id, resetCounter) {
        // Build node list from blocks, auto-inserting an Input node if none exists
        // (mirrors Windows blocks_to_snarl behavior)
        val hasInput = editState.blocks.any { it.blockType == BlockType.INPUT_ADAPTER }
        val rawBlocks = if (hasInput) {
            editState.blocks
        } else {
            listOf(dev.screengoated.toolbox.mobile.shared.preset.inputAdapter()) + editState.blocks
        }

        val seenIds = mutableSetOf<String>()
        val nodes = rawBlocks.mapIndexed { idx, block ->
            var nodeId = block.id.ifBlank { "block_$idx" }
            if (nodeId in seenIds) {
                nodeId = "${nodeId}_$idx"
            }
            seenIds.add(nodeId)
            NodePosition(
                id = nodeId,
                x = 0f,
                y = 0f,
                block = block,
            )
        }

        // Build connections directly in node-index space
        val connections = if (editState.blockConnections.isNotEmpty()) {
            // Explicit connections — shift indices if we inserted an input adapter
            val indexOffset = if (hasInput) 0 else 1
            val mapped = editState.blockConnections.mapNotNull { (from, to) ->
                val fromId = nodes.getOrNull(from + indexOffset)?.id ?: return@mapNotNull null
                val toId = nodes.getOrNull(to + indexOffset)?.id ?: return@mapNotNull null
                Connection(fromId, toId)
            }
            // If input adapter was auto-inserted, connect it to the first real block
            // (matches Windows blocks_to_snarl: virtual_input → node_ids[0])
            if (!hasInput && nodes.size >= 2) {
                listOf(Connection(nodes[0].id, nodes[1].id)) + mapped
            } else {
                mapped
            }
        } else {
            // No explicit connections — create linear chain through ALL nodes
            // (input adapter → block0 → block1 → ...)
            (0 until nodes.lastIndex).mapNotNull { i ->
                val fromId = nodes[i].id
                val toId = nodes[i + 1].id
                Connection(fromId, toId)
            }
        }

        mutableStateOf(NodeGraphState(nodes, connections))
    }

    var selectedNodeId by remember { mutableStateOf<String?>(null) }
    var showAddMenu by remember { mutableStateOf(false) }

    // Prompt edit dialog state
    var editingPromptNodeId by remember { mutableStateOf<String?>(null) }
    var editingPromptText by remember { mutableStateOf("") }

    fun syncToPreset(newGraphState: NodeGraphState) {
        graphState = newGraphState
        onUpdate(graphToPreset(newGraphState, editState))
    }

    val editorTypeGroup = editState.presetType.editorGroup()
    val canAddSpecial = editorTypeGroup != EditorTypeGroup.TEXT

    // Prompt edit dialog
    if (editingPromptNodeId != null) {
        androidx.compose.ui.window.Dialog(
            onDismissRequest = {
                val nodeId = editingPromptNodeId ?: return@Dialog
                syncToPreset(
                    graphState.copy(
                        nodes = graphState.nodes.map {
                            if (it.id == nodeId) it.copy(block = it.block.copy(prompt = editingPromptText))
                            else it
                        },
                    ),
                )
                editingPromptNodeId = null
            },
            properties = androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth = false),
        ) {
            Card(
                modifier = Modifier
                    .fillMaxWidth(0.92f)
                    .padding(16.dp),
                shape = RoundedCornerShape(20.dp),
            ) {
                Column(
                    modifier = Modifier.padding(20.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text(
                        localized(lang, "Edit Prompt", "Sửa lệnh", "프롬프트 편집"),
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                    )
                    OutlinedTextField(
                        value = editingPromptText,
                        onValueChange = { editingPromptText = it },
                        modifier = Modifier.fillMaxWidth(),
                        minLines = 3,
                        maxLines = 10,
                        textStyle = MaterialTheme.typography.bodyMedium,
                    )
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.End,
                    ) {
                        FilledTonalButton(onClick = {
                            val nodeId = editingPromptNodeId ?: return@FilledTonalButton
                            syncToPreset(
                                graphState.copy(
                                    nodes = graphState.nodes.map {
                                        if (it.id == nodeId) it.copy(block = it.block.copy(prompt = editingPromptText))
                                        else it
                                    },
                                ),
                            )
                            editingPromptNodeId = null
                        }) {
                            Text(localized(lang, "Done", "Xong", "완료"))
                        }
                    }
                }
            }
        }
    }

    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    painterResource(R.drawable.ms_account_tree),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Spacer(Modifier.width(8.dp))
                SectionLabel(localized(lang, "Node Graph", "Biểu đồ nút", "노드 그래프"))
                Spacer(Modifier.weight(1f))
                Text(
                    localized(
                        lang,
                        "Drag pin to connect",
                        "Kéo chấm để kết nối",
                        "핀을 끌어 연결",
                    ),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
                )
            }

            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(350.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .border(
                        width = 1.dp,
                        color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f),
                        shape = RoundedCornerShape(12.dp),
                    ),
            ) {
                if (graphState.nodes.isEmpty()) {
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .background(MaterialTheme.colorScheme.surfaceContainerLowest),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(
                            localized(
                                lang,
                                "No blocks \u2014 tap + to add",
                                "Chưa có khối \u2014 bấm + để thêm",
                                "블록 없음 \u2014 +를 눌러 추가",
                            ),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
                        )
                    }
                } else {
                    key(resetCounter) {
                    NodeGraphCanvas(
                        state = graphState,
                        onNodeMoved = { nodeId, x, y ->
                            syncToPreset(
                                graphState.copy(
                                    nodes = graphState.nodes.map {
                                        if (it.id == nodeId) it.copy(x = x, y = y) else it
                                    },
                                ),
                            )
                        },
                        onConnectionAdded = { fromId, toId ->
                            syncToPreset(
                                graphState.copy(
                                    connections = graphState.connections + Connection(fromId, toId),
                                ),
                            )
                        },
                        onConnectionRemoved = { fromId, toId ->
                            syncToPreset(
                                graphState.copy(
                                    connections = graphState.connections.filter {
                                        !(it.fromNodeId == fromId && it.toNodeId == toId)
                                    },
                                ),
                            )
                        },
                        onNodeDeleted = { nodeId ->
                            syncToPreset(
                                graphState.copy(
                                    nodes = graphState.nodes.filter { it.id != nodeId },
                                    connections = graphState.connections.filter {
                                        it.fromNodeId != nodeId && it.toNodeId != nodeId
                                    },
                                ),
                            )
                            if (selectedNodeId == nodeId) selectedNodeId = null
                        },
                        onBlockUpdated = { nodeId, block ->
                            syncToPreset(
                                graphState.copy(
                                    nodes = graphState.nodes.map {
                                        if (it.id == nodeId) it.copy(block = block) else it
                                    },
                                ),
                            )
                        },
                        onNodeTapped = { nodeId ->
                            selectedNodeId = if (nodeId.isNotEmpty() && selectedNodeId != nodeId) nodeId else null
                        },
                        onPromptEditRequest = { nodeId, _ ->
                            editingPromptNodeId = nodeId
                            // Read prompt from CURRENT graphState, not the stale callback param
                            editingPromptText = graphState.nodes
                                .find { it.id == nodeId }?.block?.prompt.orEmpty()
                        },
                        modifier = Modifier.fillMaxSize(),
                        lang = lang,
                        selectedNodeId = selectedNodeId,
                        presetType = editState.presetType,
                        providerSettings = providerSettings,
                    )
                    }
                }
            }

            // Add node button + hold-to-delete hint
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    localized(lang, "Tap X to delete node", "Bấm X để xóa nút", "X를 눌러 노드 삭제"),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
                )
                Spacer(Modifier.weight(1f))
                Box {
                    FilledTonalButton(onClick = { showAddMenu = true }) {
                        Icon(
                            painterResource(R.drawable.ms_add),
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                        )
                        Spacer(Modifier.width(6.dp))
                        Text(localized(lang, "Add Node", "Thêm nút", "노드 추가"))
                    }

                    DropdownMenu(
                        expanded = showAddMenu,
                        onDismissRequest = { showAddMenu = false },
                    ) {
                        DropdownMenuItem(
                            text = {
                                Text(localized(lang, "Add Text -> Text Node", "Thêm node Text -> Text", "텍스트 -> 텍스트 노드 추가"))
                            },
                            onClick = {
                                showAddMenu = false
                                val newBlock =
                                    dev.screengoated.toolbox.mobile.shared.preset.textBlock(
                                        "gemma-4-26b-a4b",
                                        "Translate to {language1}. Output ONLY the translation.",
                                        "language1" to "Vietnamese",
                                    )
                                syncToPreset(
                                    graphState.copy(
                                        nodes = graphState.nodes + NodePosition(
                                            id = "block_${System.currentTimeMillis()}",
                                            x = 0f,
                                            y = 0f,
                                            block = newBlock,
                                        ),
                                    ),
                                )
                            },
                            leadingIcon = {
                                Icon(
                                    painterResource(R.drawable.ms_text_fields),
                                    contentDescription = null,
                                    modifier = Modifier.size(18.dp),
                                )
                            },
                        )

                        if (canAddSpecial) {
                            val specialBlockType = when (editorTypeGroup) {
                                EditorTypeGroup.IMAGE -> BlockType.IMAGE
                                EditorTypeGroup.AUDIO -> BlockType.AUDIO
                                else -> BlockType.TEXT
                            }
                            val defaultModel = when (editorTypeGroup) {
                                EditorTypeGroup.IMAGE -> DEFAULT_IMAGE_MODEL_ID
                                EditorTypeGroup.AUDIO -> "whisper-accurate"
                                else -> "gemma-4-26b-a4b"
                            }
                            val specialLabel = when (editorTypeGroup) {
                                EditorTypeGroup.IMAGE -> localized(lang, "Add Image -> Text Node", "Thêm node Ảnh -> Text", "이미지 -> 텍스트 노드 추가")
                                EditorTypeGroup.AUDIO -> localized(lang, "Add Audio -> Text Node", "Thêm node Audio -> Text", "오디오 -> 텍스트 노드 추가")
                                else -> localized(lang, "Add Special Node", "Thêm node đặc biệt", "특별 노드 추가")
                            }
                            DropdownMenuItem(
                                text = { Text(specialLabel) },
                                onClick = {
                                    showAddMenu = false
                                    val newBlock =
                                        dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock(
                                            id = "special_${System.currentTimeMillis()}",
                                            blockType = specialBlockType,
                                            model = defaultModel,
                                            prompt = "",
                                        )
                                    syncToPreset(
                                        graphState.copy(
                                            nodes = graphState.nodes + NodePosition(
                                                id = newBlock.id,
                                                x = 0f,
                                                y = 0f,
                                                block = newBlock,
                                            ),
                                        ),
                                    )
                                },
                                leadingIcon = {
                                    Icon(
                                        painterResource(when (editorTypeGroup) {
                                            EditorTypeGroup.IMAGE -> R.drawable.ms_image
                                            EditorTypeGroup.AUDIO -> R.drawable.ms_mic
                                            else -> R.drawable.ms_auto_awesome
                                        }),
                                        contentDescription = null,
                                        modifier = Modifier.size(18.dp),
                                    )
                                },
                            )
                        }
                    }
                }
            }
        }
    }

}

// =============================================================================
// Section 4: Auto-paste
// =============================================================================


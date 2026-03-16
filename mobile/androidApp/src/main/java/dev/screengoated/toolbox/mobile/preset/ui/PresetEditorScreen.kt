@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.AccountTree
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.ContentPaste
import androidx.compose.material.icons.rounded.Description
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.Image
import androidx.compose.material.icons.rounded.Language
import androidx.compose.material.icons.rounded.Mic
import androidx.compose.material.icons.rounded.RestartAlt
import androidx.compose.material.icons.rounded.SpeakerPhone
import androidx.compose.material.icons.rounded.TextFields
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetType

// ---------------------------------------------------------------------------
// Editor type grouping: the 5 PresetType variants collapse into 3 editor tabs
// ---------------------------------------------------------------------------

private enum class EditorTypeGroup { IMAGE, TEXT, AUDIO }

private fun PresetType.editorGroup(): EditorTypeGroup = when (this) {
    PresetType.IMAGE -> EditorTypeGroup.IMAGE
    PresetType.TEXT_SELECT, PresetType.TEXT_INPUT -> EditorTypeGroup.TEXT
    PresetType.MIC, PresetType.DEVICE_AUDIO -> EditorTypeGroup.AUDIO
}

private fun EditorTypeGroup.defaultPresetType(): PresetType = when (this) {
    EditorTypeGroup.IMAGE -> PresetType.IMAGE
    EditorTypeGroup.TEXT -> PresetType.TEXT_SELECT
    EditorTypeGroup.AUDIO -> PresetType.MIC
}

// ---------------------------------------------------------------------------
// Localization helpers
// ---------------------------------------------------------------------------

private fun editorTypeLabel(group: EditorTypeGroup, lang: String): String = when (group) {
    EditorTypeGroup.IMAGE -> when (lang) {
        "vi" -> "Hình ảnh"
        "ko" -> "이미지"
        else -> "Image"
    }
    EditorTypeGroup.TEXT -> when (lang) {
        "vi" -> "Văn bản"
        "ko" -> "텍스트"
        else -> "Text"
    }
    EditorTypeGroup.AUDIO -> when (lang) {
        "vi" -> "Âm thanh"
        "ko" -> "오디오"
        else -> "Audio"
    }
}

private fun editorTypeIcon(group: EditorTypeGroup): ImageVector = when (group) {
    EditorTypeGroup.IMAGE -> Icons.Rounded.Image
    EditorTypeGroup.TEXT -> Icons.Rounded.TextFields
    EditorTypeGroup.AUDIO -> Icons.Rounded.GraphicEq
}

private fun localized(lang: String, en: String, vi: String, ko: String): String = when (lang) {
    "vi" -> vi
    "ko" -> ko
    else -> en
}

// ---------------------------------------------------------------------------
// Main screen
// ---------------------------------------------------------------------------

@Composable
fun PresetEditorScreen(
    preset: Preset,
    lang: String,
    onBack: () -> Unit,
) {
    val isBuiltIn = preset.id.startsWith("preset_")
    var editState by remember { mutableStateOf(preset.copy()) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        localized(lang, "Preset Editor", "Cấu hình preset", "프리셋 편집기"),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Rounded.ArrowBack, contentDescription = null)
                    }
                },
                actions = {
                    if (isBuiltIn) {
                        IconButton(onClick = { editState = preset.copy() }) {
                            Icon(
                                Icons.Rounded.RestartAlt,
                                contentDescription = localized(
                                    lang,
                                    "Restore defaults",
                                    "Khôi phục mặc định",
                                    "기본값 복원",
                                ),
                                tint = MaterialTheme.colorScheme.primary,
                            )
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surface,
                ),
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp)
                .padding(bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            // --- Section 1: Header -------------------------------------------
            HeaderSection(
                editState = editState,
                lang = lang,
                isBuiltIn = isBuiltIn,
                onNameChanged = { editState = editState.copy(nameEn = it) },
                onTypeGroupChanged = { group ->
                    val newType = group.defaultPresetType()
                    editState = editState.copy(presetType = newType)
                },
            )

            // --- Section 2: Mode selectors (conditional) ---------------------
            ModeSelectorsSection(
                editState = editState,
                lang = lang,
                onUpdate = { editState = it },
            )

            // --- Section 6 (early): Controller mode description for MASTER ---
            if (editState.isMaster) {
                MasterDescriptionSection(lang = lang)
            }

            // --- Section 3: Node graph ---------------------------------------
            if (!editState.isMaster) {
                NodeGraphSection(editState = editState, lang = lang, onUpdate = { editState = it })
            }

            // --- Section 4: Auto-paste ---------------------------------------
            AutoPasteSection(
                editState = editState,
                lang = lang,
                onUpdate = { editState = it },
            )

            // --- Section 5: Processing chain list ----------------------------
            if (editState.blocks.isNotEmpty()) {
                ProcessingChainSection(
                    editState = editState,
                    lang = lang,
                )
            }
        }
    }
}

// =============================================================================
// Section 1: Header
// =============================================================================

@Composable
private fun HeaderSection(
    editState: Preset,
    lang: String,
    isBuiltIn: Boolean,
    onNameChanged: (String) -> Unit,
    onTypeGroupChanged: (EditorTypeGroup) -> Unit,
) {
    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
            // Preset name
            SectionLabel(localized(lang, "Name", "Tên", "이름"))

            if (isBuiltIn) {
                Text(
                    text = editState.name(lang),
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            } else {
                OutlinedTextField(
                    value = editState.nameEn,
                    onValueChange = onNameChanged,
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    label = {
                        Text(localized(lang, "Preset name", "Tên preset", "프리셋 이름"))
                    },
                )
            }

            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))

            // Preset type
            SectionLabel(localized(lang, "Type", "Loại", "유형"))

            val currentGroup = editState.presetType.editorGroup()
            val groups = EditorTypeGroup.entries

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
            ) {
                groups.forEachIndexed { index, group ->
                    ToggleButton(
                        checked = currentGroup == group,
                        onCheckedChange = { if (it) onTypeGroupChanged(group) },
                        shapes = when (index) {
                            0 -> ButtonGroupDefaults.connectedLeadingButtonShapes()
                            groups.lastIndex -> ButtonGroupDefaults.connectedTrailingButtonShapes()
                            else -> ButtonGroupDefaults.connectedMiddleButtonShapes()
                        },
                        modifier = Modifier
                            .weight(1f)
                            .semantics { role = Role.RadioButton },
                    ) {
                        Icon(
                            editorTypeIcon(group),
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(4.dp))
                        Text(
                            editorTypeLabel(group, lang),
                            style = MaterialTheme.typography.labelMedium,
                        )
                    }
                }
            }
        }
    }
}

// =============================================================================
// Section 2: Mode selectors
// =============================================================================

@Composable
private fun ModeSelectorsSection(
    editState: Preset,
    lang: String,
    onUpdate: (Preset) -> Unit,
) {
    val group = editState.presetType.editorGroup()

    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
            SectionLabel(localized(lang, "Mode", "Chế độ", "모드"))

            when (group) {
                EditorTypeGroup.IMAGE -> ImageModeSelectors(editState, lang, onUpdate)
                EditorTypeGroup.TEXT -> TextModeSelectors(editState, lang, onUpdate)
                EditorTypeGroup.AUDIO -> AudioModeSelectors(editState, lang, onUpdate)
            }
        }
    }
}

@Composable
private fun ImageModeSelectors(
    editState: Preset,
    lang: String,
    onUpdate: (Preset) -> Unit,
) {
    // Command mode: Fixed / Dynamic
    TogglePair(
        label = localized(lang, "Command mode", "Chế độ lệnh", "명령 모드"),
        optionA = localized(lang, "Fixed", "Cố định", "고정"),
        optionB = localized(lang, "Dynamic", "Linh hoạt", "동적"),
        isB = editState.promptMode == "dynamic",
        onChanged = { isDynamic ->
            onUpdate(editState.copy(promptMode = if (isDynamic) "dynamic" else "fixed"))
        },
    )
}

@Composable
private fun TextModeSelectors(
    editState: Preset,
    lang: String,
    onUpdate: (Preset) -> Unit,
) {
    val isInputMode = editState.presetType == PresetType.TEXT_INPUT

    // Input mode: Select / Type
    TogglePair(
        label = localized(lang, "Input mode", "Chế độ nhập", "입력 모드"),
        optionA = localized(lang, "Select", "Chọn", "선택"),
        optionB = localized(lang, "Type", "Nhập", "입력"),
        isB = isInputMode,
        onChanged = { isType ->
            val newType = if (isType) PresetType.TEXT_INPUT else PresetType.TEXT_SELECT
            onUpdate(editState.copy(presetType = newType))
        },
    )

    // Conditional sub-options
    AnimatedVisibility(
        visible = isInputMode,
        enter = fadeIn() + expandVertically(),
        exit = fadeOut() + shrinkVertically(),
    ) {
        SwitchRow(
            label = localized(lang, "Continuous input", "Nhập liên tục", "연속 입력"),
            checked = editState.continuousInput,
            onCheckedChange = { onUpdate(editState.copy(continuousInput = it)) },
        )
    }

    AnimatedVisibility(
        visible = !isInputMode,
        enter = fadeIn() + expandVertically(),
        exit = fadeOut() + shrinkVertically(),
    ) {
        TogglePair(
            label = localized(lang, "Command mode", "Chế độ lệnh", "명령 모드"),
            optionA = localized(lang, "Fixed", "Cố định", "고정"),
            optionB = localized(lang, "Dynamic", "Linh hoạt", "동적"),
            isB = editState.promptMode == "dynamic",
            onChanged = { isDynamic ->
                onUpdate(editState.copy(promptMode = if (isDynamic) "dynamic" else "fixed"))
            },
        )
    }
}

@Composable
private fun AudioModeSelectors(
    editState: Preset,
    lang: String,
    onUpdate: (Preset) -> Unit,
) {
    // Processing mode: Record / Realtime
    val isRealtime = editState.presetType == PresetType.DEVICE_AUDIO &&
        editState.audioSource == "device"

    TogglePair(
        label = localized(lang, "Processing mode", "Chế độ xử lý", "처리 모드"),
        optionA = localized(lang, "Record", "Ghi âm", "녹음"),
        optionB = localized(lang, "Realtime", "Thời gian thực", "실시간"),
        isB = isRealtime,
        onChanged = { /* future: wire up processing mode */ },
    )

    // Audio source: Mic / Device
    val isMic = editState.presetType == PresetType.MIC || editState.audioSource == "mic"

    TogglePair(
        label = localized(lang, "Audio source", "Nguồn âm thanh", "오디오 소스"),
        optionA = localized(lang, "Mic", "Mic", "마이크"),
        optionB = localized(lang, "Device", "Thiết bị", "기기"),
        isB = !isMic,
        iconA = Icons.Rounded.Mic,
        iconB = Icons.Rounded.SpeakerPhone,
        onChanged = { isDevice ->
            val newType = if (isDevice) PresetType.DEVICE_AUDIO else PresetType.MIC
            val newSource = if (isDevice) "device" else "mic"
            onUpdate(editState.copy(presetType = newType, audioSource = newSource))
        },
    )

    // Auto-stop recording
    SwitchRow(
        label = localized(lang, "Auto-stop recording", "Tự động dừng ghi", "자동 녹음 중지"),
        checked = editState.autoStopRecording,
        onCheckedChange = { onUpdate(editState.copy(autoStopRecording = it)) },
    )
}

// =============================================================================
// Section 3: Node graph
// =============================================================================

/** Convert graph state back to preset blocks + connections (BFS from input node). */
private fun graphToPreset(graphState: NodeGraphState, currentPreset: Preset): Preset {
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
private fun NodeGraphSection(
    editState: Preset,
    lang: String,
    onUpdate: (Preset) -> Unit,
) {
    var graphState by remember(editState.id) {
        // Build node list from blocks, auto-inserting an Input node if none exists
        // (mirrors Windows blocks_to_snarl behavior)
        val hasInput = editState.blocks.any { it.blockType == BlockType.INPUT_ADAPTER }
        val rawBlocks = if (hasInput) {
            editState.blocks
        } else {
            listOf(dev.screengoated.toolbox.mobile.shared.preset.inputAdapter()) + editState.blocks
        }

        val nodes = rawBlocks.mapIndexed { idx, block ->
            NodePosition(
                id = block.id.ifBlank { "block_$idx" },
                x = 0f,
                y = 0f,
                block = block,
            )
        }

        // Normalize connections: if blockConnections is empty, create linear chain
        // (same as Preset.normalizedConnections() in PresetRepository)
        val indexOffset = if (hasInput) 0 else 1
        val sourceEdges = if (editState.blockConnections.isNotEmpty()) {
            editState.blockConnections
        } else if (editState.blocks.size >= 2) {
            // Linear chain: 0→1→2→...
            (0 until editState.blocks.lastIndex).map { it to it + 1 }
        } else {
            emptyList()
        }

        val connections = if (hasInput) {
            sourceEdges.mapNotNull { (from, to) ->
                val fromId = nodes.getOrNull(from)?.id ?: return@mapNotNull null
                val toId = nodes.getOrNull(to)?.id ?: return@mapNotNull null
                Connection(fromId, toId)
            }
        } else {
            // Shift original connections by 1 (we prepended an input adapter)
            val shifted = sourceEdges.mapNotNull { (from, to) ->
                val fromId = nodes.getOrNull(from + indexOffset)?.id ?: return@mapNotNull null
                val toId = nodes.getOrNull(to + indexOffset)?.id ?: return@mapNotNull null
                Connection(fromId, toId)
            }
            // Auto-connect input to all nodes with no incoming edge
            val hasIncoming = shifted.map { it.toNodeId }.toSet()
            val inputId = nodes.first().id
            val autoConns = nodes.drop(1)
                .filter { it.id !in hasIncoming }
                .map { Connection(inputId, it.id) }
            autoConns + shifted
        }

        mutableStateOf(NodeGraphState(nodes, connections))
    }

    var selectedNodeId by remember { mutableStateOf<String?>(null) }
    var showPropertySheet by remember { mutableStateOf(false) }
    var showAddMenu by remember { mutableStateOf(false) }

    fun syncToPreset(newGraphState: NodeGraphState) {
        graphState = newGraphState
        onUpdate(graphToPreset(newGraphState, editState))
    }

    val editorTypeGroup = editState.presetType.editorGroup()
    val canAddSpecial = editorTypeGroup != EditorTypeGroup.TEXT

    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    Icons.Rounded.AccountTree,
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
                        "Tap node to edit",
                        "Chạm nút để sửa",
                        "노드 탭하여 편집",
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
                            if (nodeId.isNotEmpty()) {
                                selectedNodeId = nodeId
                                showPropertySheet = true
                            } else {
                                selectedNodeId = null
                            }
                        },
                        modifier = Modifier.fillMaxSize(),
                        lang = lang,
                        selectedNodeId = selectedNodeId,
                    )
                }
            }

            // Add node button with dropdown
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
            ) {
                Box {
                    FilledTonalButton(onClick = { showAddMenu = true }) {
                        Icon(
                            Icons.Rounded.Add,
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
                                Text(localized(lang, "Process Node", "Nút xử lý", "처리 노드"))
                            },
                            onClick = {
                                showAddMenu = false
                                val newBlock =
                                    dev.screengoated.toolbox.mobile.shared.preset.textBlock(
                                        "cerebras_gpt_oss",
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
                                    Icons.Rounded.TextFields,
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
                                EditorTypeGroup.IMAGE -> "gemini-3.1-flash-lite-preview"
                                EditorTypeGroup.AUDIO -> "whisper-accurate"
                                else -> "cerebras_gpt_oss"
                            }
                            DropdownMenuItem(
                                text = {
                                    Text(
                                        localized(
                                            lang,
                                            "Special Node",
                                            "Nút đặc biệt",
                                            "특수 노드",
                                        ),
                                    )
                                },
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
                                        when (editorTypeGroup) {
                                            EditorTypeGroup.IMAGE -> Icons.Rounded.Image
                                            EditorTypeGroup.AUDIO -> Icons.Rounded.Mic
                                            else -> Icons.Rounded.AutoAwesome
                                        },
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

    // Property sheet bottom sheet
    if (showPropertySheet && selectedNodeId != null) {
        val selectedNode = graphState.nodes.find { it.id == selectedNodeId }
        if (selectedNode != null) {
            NodePropertySheet(
                block = selectedNode.block,
                nodeId = selectedNode.id,
                lang = lang,
                onDismiss = { showPropertySheet = false },
                onBlockUpdated = { updatedBlock ->
                    val newNodes = graphState.nodes.map { node ->
                        when {
                            node.id == selectedNodeId -> node.copy(block = updatedBlock)
                            updatedBlock.autoCopy && node.block.autoCopy ->
                                node.copy(block = node.block.copy(autoCopy = false))
                            else -> node
                        }
                    }
                    syncToPreset(graphState.copy(nodes = newNodes))
                },
            )
        }
    }
}

// =============================================================================
// Section 4: Auto-paste
// =============================================================================

@Composable
private fun AutoPasteSection(
    editState: Preset,
    lang: String,
    onUpdate: (Preset) -> Unit,
) {
    var autoPasteNewline by remember { mutableStateOf(false) }

    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    Icons.Rounded.ContentPaste,
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Spacer(Modifier.width(8.dp))
                SectionLabel(localized(lang, "Auto-Paste", "Tự động dán", "자동 붙여넣기"))
            }

            SwitchRow(
                label = localized(lang, "Auto-paste output", "Tự động dán kết quả", "출력 자동 붙여넣기"),
                checked = editState.autoPaste,
                onCheckedChange = { onUpdate(editState.copy(autoPaste = it)) },
            )

            AnimatedVisibility(
                visible = editState.autoPaste,
                enter = fadeIn() + expandVertically(),
                exit = fadeOut() + shrinkVertically(),
            ) {
                SwitchRow(
                    label = localized(
                        lang,
                        "Append newline",
                        "Thêm dòng mới",
                        "줄 바꿈 추가",
                    ),
                    checked = autoPasteNewline,
                    onCheckedChange = { autoPasteNewline = it },
                )
            }
        }
    }
}

// =============================================================================
// Section 5: Processing chain list
// =============================================================================

@Composable
private fun ProcessingChainSection(
    editState: Preset,
    lang: String,
) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(
            modifier = Modifier.padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                Icons.Rounded.AutoAwesome,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
            Spacer(Modifier.width(8.dp))
            SectionLabel(
                localized(lang, "Processing Chain", "Chuỗi xử lý", "처리 체인"),
            )
            Spacer(Modifier.weight(1f))
            Text(
                "${editState.blocks.size} ${localized(lang, "blocks", "khối", "블록")}",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        editState.blocks.forEachIndexed { idx, block ->
            ProcessingBlockCard(idx = idx, block = block, lang = lang)
        }

        // Connections
        if (editState.blockConnections.isNotEmpty()) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
                ),
            ) {
                Row(
                    modifier = Modifier.padding(12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(
                        Icons.Rounded.AccountTree,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        editState.blockConnections.joinToString("  \u2192  ") { (from, to) ->
                            "${from + 1} \u2192 ${to + 1}"
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

@Composable
private fun ProcessingBlockCard(
    idx: Int,
    block: dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock,
    lang: String,
) {
    val blockColor = when (block.blockType) {
        BlockType.INPUT_ADAPTER -> MaterialTheme.colorScheme.surfaceContainerLow
        BlockType.IMAGE -> MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.25f)
        BlockType.TEXT -> MaterialTheme.colorScheme.secondaryContainer.copy(alpha = 0.25f)
        BlockType.AUDIO -> MaterialTheme.colorScheme.tertiaryContainer.copy(alpha = 0.25f)
    }

    val badgeColor = when (block.blockType) {
        BlockType.INPUT_ADAPTER -> MaterialTheme.colorScheme.outline
        BlockType.IMAGE -> MaterialTheme.colorScheme.primary
        BlockType.TEXT -> MaterialTheme.colorScheme.secondary
        BlockType.AUDIO -> MaterialTheme.colorScheme.tertiary
    }

    val blockLabel = when (block.blockType) {
        BlockType.INPUT_ADAPTER -> localized(lang, "Input", "Đầu vào", "입력")
        BlockType.IMAGE -> localized(lang, "Image", "Hình ảnh", "이미지")
        BlockType.TEXT -> localized(lang, "Text", "Văn bản", "텍스트")
        BlockType.AUDIO -> localized(lang, "Audio", "Âm thanh", "오디오")
    }

    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = blockColor),
        shape = MaterialTheme.shapes.medium,
    ) {
        Column(
            modifier = Modifier.padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            // Header row: badge + model
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                // Type badge
                Surface(
                    shape = RoundedCornerShape(6.dp),
                    color = badgeColor.copy(alpha = 0.15f),
                ) {
                    Text(
                        text = blockLabel,
                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp),
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.SemiBold,
                        color = badgeColor,
                    )
                }

                // Block number
                Surface(
                    shape = CircleShape,
                    color = MaterialTheme.colorScheme.surfaceContainerHighest,
                    modifier = Modifier.size(22.dp),
                ) {
                    Box(contentAlignment = Alignment.Center, modifier = Modifier.fillMaxSize()) {
                        Text(
                            "${idx + 1}",
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.Bold,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }

                Spacer(Modifier.weight(1f))

                // Model name
                if (block.model.isNotBlank()) {
                    Text(
                        block.model,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.primary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }

            // Prompt preview
            if (block.prompt.isNotBlank()) {
                Text(
                    block.prompt,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                )
            }

            // Language variables
            if (block.languageVars.isNotEmpty()) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(
                        Icons.Rounded.Language,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = MaterialTheme.colorScheme.tertiary,
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        block.languageVars.entries.joinToString(" \u00b7 ") { "${it.key}=${it.value}" },
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.tertiary,
                    )
                }
            }

            // Auto-behaviors row
            val behaviors = buildList {
                if (block.autoCopy) add(
                    localized(lang, "Auto-copy", "Tự sao chép", "자동 복사") to Icons.Rounded.ContentCopy
                )
                if (block.autoSpeak) add(
                    localized(lang, "Auto-speak", "Tự phát âm", "자동 말하기") to Icons.Rounded.GraphicEq
                )
                if (block.streamingEnabled) add(
                    localized(lang, "Streaming", "Phát trực tuyến", "스트리밍") to Icons.Rounded.AutoAwesome
                )
            }
            if (behaviors.isNotEmpty()) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    behaviors.forEach { (label, icon) ->
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(
                                icon,
                                contentDescription = null,
                                modifier = Modifier.size(12.dp),
                                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            Spacer(Modifier.width(3.dp))
                            Text(
                                label,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }
        }
    }
}

// =============================================================================
// Section 6: Master preset description
// =============================================================================

@Composable
private fun MasterDescriptionSection(lang: String) {
    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    Icons.Rounded.Description,
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Spacer(Modifier.width(8.dp))
                SectionLabel(
                    localized(lang, "Controller Mode", "Chế độ điều khiển", "컨트롤러 모드"),
                )
            }

            Text(
                localized(
                    lang,
                    "This is a master preset that controls the execution flow of other presets. " +
                        "It does not process input directly but orchestrates multiple processing chains " +
                        "to produce a combined result.",
                    "Đây là preset chính điều khiển luồng thực thi của các preset khác. " +
                        "Nó không xử lý đầu vào trực tiếp mà điều phối nhiều chuỗi xử lý " +
                        "để tạo ra kết quả kết hợp.",
                    "이것은 다른 프리셋의 실행 흐름을 제어하는 마스터 프리셋입니다. " +
                        "입력을 직접 처리하지 않고 여러 처리 체인을 조율하여 " +
                        "결합된 결과를 생성합니다.",
                ),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

// =============================================================================
// Reusable components
// =============================================================================

@Composable
private fun SectionCard(content: @Composable () -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
        shape = MaterialTheme.shapes.large,
    ) {
        Box(modifier = Modifier.padding(16.dp)) {
            content()
        }
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(
        text = text,
        style = MaterialTheme.typography.titleSmall,
        fontWeight = FontWeight.Bold,
        color = MaterialTheme.colorScheme.onSurface,
    )
}

@Composable
private fun SwitchRow(
    label: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f),
        )
        Switch(
            checked = checked,
            onCheckedChange = onCheckedChange,
        )
    }
}

@Composable
private fun TogglePair(
    label: String,
    optionA: String,
    optionB: String,
    isB: Boolean,
    onChanged: (Boolean) -> Unit,
    iconA: ImageVector? = null,
    iconB: ImageVector? = null,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
        ) {
            ToggleButton(
                checked = !isB,
                onCheckedChange = { if (it) onChanged(false) },
                shapes = ButtonGroupDefaults.connectedLeadingButtonShapes(),
                modifier = Modifier
                    .weight(1f)
                    .semantics { role = Role.RadioButton },
            ) {
                if (iconA != null) {
                    Icon(iconA, contentDescription = null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(4.dp))
                }
                Text(optionA, style = MaterialTheme.typography.labelMedium)
            }
            ToggleButton(
                checked = isB,
                onCheckedChange = { if (it) onChanged(true) },
                shapes = ButtonGroupDefaults.connectedTrailingButtonShapes(),
                modifier = Modifier
                    .weight(1f)
                    .semantics { role = Role.RadioButton },
            ) {
                if (iconB != null) {
                    Icon(iconB, contentDescription = null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(4.dp))
                }
                Text(optionB, style = MaterialTheme.typography.labelMedium)
            }
        }
    }
}

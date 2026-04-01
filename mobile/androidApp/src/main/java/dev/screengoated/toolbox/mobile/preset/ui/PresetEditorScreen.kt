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

// Google Sans Flex at wdth=80 — prevents long labels from wrapping in toggle buttons
private val condensedButtonFont: androidx.compose.ui.text.font.FontFamily by lazy {
    if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
        androidx.compose.ui.text.font.FontFamily(
            androidx.compose.ui.text.font.Font(
                resId = dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
                variationSettings = androidx.compose.ui.text.font.FontVariation.Settings(
                    androidx.compose.ui.text.font.FontVariation.Setting("wdth", 80f),
                ),
            ),
        )
    } else {
        androidx.compose.ui.text.font.FontFamily.Default
    }
}

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
        "vi" -> "Ảnh"
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

@DrawableRes
private fun editorTypeIcon(group: EditorTypeGroup): Int = when (group) {
    EditorTypeGroup.IMAGE -> R.drawable.ms_image
    EditorTypeGroup.TEXT -> R.drawable.ms_text_fields
    EditorTypeGroup.AUDIO -> R.drawable.ms_audio_file
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
    onPresetChanged: (Preset) -> Unit = {},
    onRestoreDefault: () -> Unit = {},
    providerSettings: dev.screengoated.toolbox.mobile.preset.PresetProviderSettings =
        dev.screengoated.toolbox.mobile.preset.PresetProviderSettings(),
) {
    val isBuiltIn = preset.id.startsWith("preset_")
    var editState by remember(preset) { mutableStateOf(preset.copy()) }
    var resetCounter by remember { mutableStateOf(0) }

    fun autoSave(newState: Preset) {
        editState = newState
        onPresetChanged(newState)
    }

    var isRenamingPreset by remember { mutableStateOf(false) }
    var renameText by remember(editState.nameEn) { mutableStateOf(editState.nameEn) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    if (isRenamingPreset && !isBuiltIn) {
                        OutlinedTextField(
                            value = renameText,
                            onValueChange = { renameText = it },
                            singleLine = true,
                            modifier = Modifier.fillMaxWidth(),
                            textStyle = MaterialTheme.typography.titleMedium,
                            trailingIcon = {
                                IconButton(onClick = {
                                    autoSave(editState.copy(nameEn = renameText))
                                    isRenamingPreset = false
                                }) { Icon(painterResource(R.drawable.ms_check), contentDescription = null) }
                            },
                        )
                    } else {
                        Text(
                            text = editState.name(lang),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = if (!isBuiltIn) Modifier.clickable { isRenamingPreset = true } else Modifier,
                        )
                    }
                },
                navigationIcon = {
                    IconButton(onClick = { if (isRenamingPreset) isRenamingPreset = false else onBack() }) {
                        Icon(painterResource(R.drawable.ms_arrow_back), contentDescription = null)
                    }
                },
                actions = {
                    if (isBuiltIn) {
                        IconButton(onClick = {
                            onRestoreDefault()
                            resetCounter++
                        }) {
                            Icon(painterResource(R.drawable.ms_settings_backup_restore), contentDescription = localized(lang, "Restore", "Khôi phục", "복원"), tint = MaterialTheme.colorScheme.primary)
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.surface),
            )
        },
    ) { padding ->
        val configuration = androidx.compose.ui.platform.LocalConfiguration.current
        val isLandscape = configuration.screenWidthDp > configuration.screenHeightDp

        if (isLandscape) {
            Row(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .padding(horizontal = 16.dp)
                    .padding(bottom = 16.dp),
                horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                // Left column: Header + Mode selectors
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    HeaderSection(
                        editState = editState,
                        lang = lang,
                        isBuiltIn = isBuiltIn,
                        onNameChanged = { autoSave(editState.copy(nameEn = it)) },
                        onTypeGroupChanged = { group ->
                            autoSave(editState.copy(presetType = group.defaultPresetType()))
                        },
                        onControllerToggled = { autoSave(editState.copy(showControllerUi = it)) },
                    )
                    ModeSelectorsSection(
                        editState = editState,
                        lang = lang,
                        controllerOn = editState.showControllerUi || editState.isMaster,
                        onUpdate = { autoSave(it) },
                    )
                    if (editState.showControllerUi || editState.isMaster) {
                        MasterDescriptionSection(lang = lang)
                    }
                    if (editState.audioProcessingMode == "realtime") {
                        RealtimeDescriptionSection(lang = lang)
                    }
                }
                // Right column: Node graph + Processing chain
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    if (!editState.showControllerUi && !editState.isMaster && editState.audioProcessingMode != "realtime") {
                        NodeGraphSection(
                            editState = editState,
                            lang = lang,
                            onUpdate = { autoSave(it) },
                            resetCounter = resetCounter,
                            providerSettings = providerSettings,
                        )
                    }
                    val hasAnyCopy = editState.blocks.any {
                        it.blockType != BlockType.INPUT_ADAPTER && it.autoCopy
                    }
                    if (hasAnyCopy && !editState.showControllerUi) {
                        AutoPasteSection(
                            editState = editState,
                            lang = lang,
                            onUpdate = { autoSave(it) },
                        )
                    }
                }
            }
        } else {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp)
                    .padding(bottom = 32.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                HeaderSection(
                    editState = editState,
                    lang = lang,
                    isBuiltIn = isBuiltIn,
                    onNameChanged = { autoSave(editState.copy(nameEn = it)) },
                    onTypeGroupChanged = { group ->
                        autoSave(editState.copy(presetType = group.defaultPresetType()))
                    },
                    onControllerToggled = { autoSave(editState.copy(showControllerUi = it)) },
                )
                ModeSelectorsSection(
                    editState = editState,
                    lang = lang,
                    controllerOn = editState.showControllerUi || editState.isMaster,
                    onUpdate = { autoSave(it) },
                )
                if (editState.showControllerUi || editState.isMaster) {
                    MasterDescriptionSection(lang = lang)
                }
                if (editState.audioProcessingMode == "realtime") {
                    RealtimeDescriptionSection(lang = lang)
                }
                if (!editState.showControllerUi && !editState.isMaster && editState.audioProcessingMode != "realtime") {
                    NodeGraphSection(
                        editState = editState,
                        lang = lang,
                        onUpdate = { autoSave(it) },
                        resetCounter = resetCounter,
                        providerSettings = providerSettings,
                    )
                }
                val hasAnyCopy = editState.blocks.any {
                    it.blockType != BlockType.INPUT_ADAPTER && it.autoCopy
                }
                if (hasAnyCopy && !editState.showControllerUi) {
                    AutoPasteSection(
                        editState = editState,
                        lang = lang,
                        onUpdate = { autoSave(it) },
                    )
                }
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
    onControllerToggled: (Boolean) -> Unit,
) {
    // Type selector card
    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
            SectionLabel(localized(lang, "Type", "Loại hình", "유형"))

            val currentGroup = editState.presetType.editorGroup()
            val groups = EditorTypeGroup.entries

            FlowRow(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
                verticalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
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
                            painterResource(editorTypeIcon(group)),
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(4.dp))
                        Text(
                            editorTypeLabel(group, lang),
                            style = MaterialTheme.typography.labelMedium,
                            fontFamily = condensedButtonFont,
                            maxLines = 1,
                            softWrap = false,
                        )
                    }
                }
            }
        }
    }

    // Controller toggle — separate card, hidden for realtime audio
    val isRealtimeAudio = editState.presetType.editorGroup() == EditorTypeGroup.AUDIO &&
        editState.audioProcessingMode == "realtime"
    if (!isRealtimeAudio) {
        SectionCard {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                SectionLabel(localized(lang, "Controller", "Bộ điều khiển", "컨트롤러"))
                Spacer(Modifier.weight(1f))
                Switch(
                    checked = editState.showControllerUi,
                    onCheckedChange = onControllerToggled,
                )
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
    controllerOn: Boolean = false,
    onUpdate: (Preset) -> Unit,
) {
    val group = editState.presetType.editorGroup()
    val hideEntireSection = group == EditorTypeGroup.IMAGE && controllerOn
    if (!hideEntireSection) {
        SectionCard {
            Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                when (group) {
                    EditorTypeGroup.IMAGE -> ImageModeSelectors(editState, lang, onUpdate)
                    EditorTypeGroup.TEXT -> TextModeSelectors(editState, lang, controllerOn, onUpdate)
                    EditorTypeGroup.AUDIO -> AudioModeSelectors(editState, lang, controllerOn, onUpdate)
                }
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
    TogglePair(
        label = localized(lang, "Command", "Lệnh", "명령"),
        optionA = localized(lang, "Predefined Prompt", "Làm theo lệnh sẵn", "사전 정의된 프롬프트"),
        optionB = localized(lang, "Write on the spot", "Viết lệnh tại chỗ", "즉석에서 작성"),
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
    controllerOn: Boolean = false,
    onUpdate: (Preset) -> Unit,
) {
    val isInputMode = editState.presetType == PresetType.TEXT_INPUT

    TogglePair(
        label = localized(lang, "Mode", "Phương thức", "작동 방식"),
        optionA = localized(lang, "Select text", "Bôi text", "텍스트 선택"),
        optionB = localized(lang, "Type", "Gõ text", "입력"),
        isB = isInputMode,
        onChanged = { isType ->
            val newType = if (isType) PresetType.TEXT_INPUT else PresetType.TEXT_SELECT
            onUpdate(editState.copy(presetType = newType))
        },
    )

    if (!controllerOn) {
        AnimatedVisibility(visible = isInputMode, enter = fadeIn() + expandVertically(), exit = fadeOut() + shrinkVertically()) {
            SwitchRow(
                label = localized(lang, "Continuous input", "Nhập liên tục", "연속 입력"),
                checked = editState.continuousInput,
                onCheckedChange = { onUpdate(editState.copy(continuousInput = it)) },
            )
        }
        AnimatedVisibility(visible = !isInputMode, enter = fadeIn() + expandVertically(), exit = fadeOut() + shrinkVertically()) {
            TogglePair(
                label = localized(lang, "Command", "Lệnh", "명령"),
                optionA = localized(lang, "Predefined Prompt", "Làm theo lệnh sẵn", "사전 정의된 프롬프트"),
                optionB = localized(lang, "Write on the spot", "Viết lệnh tại chỗ", "즉석에서 작성"),
                isB = editState.promptMode == "dynamic",
                onChanged = { isDynamic -> onUpdate(editState.copy(promptMode = if (isDynamic) "dynamic" else "fixed")) },
            )
        }
    }
}

@Composable
private fun AudioModeSelectors(
    editState: Preset,
    lang: String,
    controllerOn: Boolean = false,
    onUpdate: (Preset) -> Unit,
) {
    val isRealtime = editState.audioProcessingMode == "realtime"

    // Audio source — hidden for realtime (always device audio)
    if (!isRealtime) {
        val isMic = editState.presetType == PresetType.MIC || editState.audioSource == "mic"
        TogglePair(
            label = localized(lang, "Audio Source", "Nguồn", "오디오 소스"),
            optionA = localized(lang, "Microphone", "Microphone", "마이크"),
            optionB = localized(lang, "Device Audio", "Âm thanh máy tính", "컴퓨터 오디오"),
            isB = !isMic,
            iconA = R.drawable.ms_mic,
            iconB = R.drawable.ms_speaker_phone,
            onChanged = { isDevice ->
                val newType = if (isDevice) PresetType.DEVICE_AUDIO else PresetType.MIC
                val newSource = if (isDevice) "device" else "mic"
                onUpdate(editState.copy(presetType = newType, audioSource = newSource))
            },
        )
    }

    if (controllerOn) return

    // Processing mode
    TogglePair(
        label = localized(lang, "Mode", "Phương thức", "작동 방식"),
        optionA = localized(lang, "Record then Process", "Thu âm rồi xử lý", "녹음 후 처리"),
        optionB = localized(lang, "Realtime Processing", "Xử lý thời gian thực", "실시간 처리"),
        isB = isRealtime,
        onChanged = { isRealtimeMode ->
            if (isRealtimeMode) {
                onUpdate(editState.copy(
                    presetType = PresetType.DEVICE_AUDIO,
                    audioSource = "device",
                    audioProcessingMode = "realtime",
                ))
            } else {
                onUpdate(editState.copy(audioProcessingMode = "record_then_process"))
            }
        },
    )

    // Auto-stop and other options hidden for realtime
    if (isRealtime) return

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
                                else -> "cerebras_gpt_oss"
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

@Composable
private fun AutoPasteSection(
    editState: Preset,
    lang: String,
    onUpdate: (Preset) -> Unit,
) {
    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    painterResource(R.drawable.ms_content_paste),
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
                    label = localized(lang, "Append newline", "Thêm dòng mới", "줄 바꿈 추가"),
                    checked = editState.autoPasteNewline,
                    onCheckedChange = { onUpdate(editState.copy(autoPasteNewline = it)) },
                )
            }
        }
    }
}

// =============================================================================
// Section 5: Master preset description
// =============================================================================

@Composable
private fun MasterDescriptionSection(lang: String) {
    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    painterResource(R.drawable.ms_description),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Spacer(Modifier.width(8.dp))
                SectionLabel(
                    localized(lang, "Controller", "Bộ điều khiển", "컨트롤러"),
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

@Composable
private fun RealtimeDescriptionSection(lang: String) {
    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    painterResource(R.drawable.ms_audio_file),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Spacer(Modifier.width(8.dp))
                SectionLabel(
                    localized(lang,
                        "Realtime Audio Processing",
                        "Xử lý âm thanh (Thời gian thực)",
                        "실시간 오디오 처리",
                    ),
                )
            }

            Text(
                localized(
                    lang,
                    "This mode provides real-time transcription and translation.\n" +
                        "Gemini API key is required, works best on audio with clear speech like podcasts!\n\n" +
                        "You can adjust font size, audio source, and translation language directly in the result window.",
                    "Chế độ này cung cấp phụ đề và dịch thuật trực tiếp theo thời gian thực.\n" +
                        "Mã API của Gemini là bắt buộc, tính năng chỉ hoạt động tốt trên âm thanh có lời nói to rõ như podcast!\n\n" +
                        "Bạn có thể điều chỉnh cỡ chữ, nguồn âm thanh và ngôn ngữ dịch ngay trong cửa sổ kết quả.",
                    "이 모드는 실시간 자막 및 번역을 제공합니다.\n" +
                        "Gemini API 키가 필수이며, 명확한 음성이 있는 팟캐스트 같은 오디오에서 잘 작동합니다!\n\n" +
                        "결과 창에서 글꼴 크기, 오디오 소스, 번역 언어를 직접 조정할 수 있습니다.",
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
    @DrawableRes iconA: Int? = null,
    @DrawableRes iconB: Int? = null,
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
                    Icon(painterResource(iconA), contentDescription = null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(4.dp))
                }
                Text(optionA, style = MaterialTheme.typography.labelMedium,
                    fontFamily = condensedButtonFont, maxLines = 1, softWrap = false)
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
                    Icon(painterResource(iconB), contentDescription = null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(4.dp))
                }
                Text(optionB, style = MaterialTheme.typography.labelMedium,
                    fontFamily = condensedButtonFont, maxLines = 1, softWrap = false)
            }
        }
    }
}

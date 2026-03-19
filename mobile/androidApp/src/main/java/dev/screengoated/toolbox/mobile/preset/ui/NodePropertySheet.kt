@file:OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.Language
import androidx.compose.material.icons.rounded.RemoveRedEye
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.VolumeUp
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.ToggleButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock

// ---------------------------------------------------------------------------
// Localization helpers
// ---------------------------------------------------------------------------

private fun l(lang: String, en: String, vi: String, ko: String): String = when (lang) {
    "vi" -> vi
    "ko" -> ko
    else -> en
}

// ---------------------------------------------------------------------------
// Render mode model
// ---------------------------------------------------------------------------

private data class RenderModeOption(
    val label: String,
    val renderMode: String,
    val streaming: Boolean,
)

private fun renderModeOptions(lang: String): List<RenderModeOption> = listOf(
    RenderModeOption(l(lang, "Normal", "Thường", "일반"), "plain", false),
    RenderModeOption(l(lang, "Stream", "Luồng", "스트림"), "stream", true),
    RenderModeOption(l(lang, "Markdown", "Đẹp", "마크다운"), "markdown", false),
    RenderModeOption(l(lang, "MD+Stream", "Đẹp+Luồng", "마크+스트림"), "markdown_stream", true),
)

private fun currentRenderModeIndex(block: ProcessingBlock): Int {
    return when {
        block.renderMode == "plain" && !block.streamingEnabled -> 0
        block.renderMode == "stream" && block.streamingEnabled -> 1
        block.renderMode == "markdown" && !block.streamingEnabled -> 2
        block.renderMode == "markdown_stream" && block.streamingEnabled -> 3
        else -> 0
    }
}

// ---------------------------------------------------------------------------
// Main bottom sheet
// ---------------------------------------------------------------------------

@Composable
fun NodePropertySheet(
    block: ProcessingBlock,
    nodeId: String,
    lang: String,
    presetType: dev.screengoated.toolbox.mobile.shared.preset.PresetType =
        dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT,
    onDismiss: () -> Unit,
    onBlockUpdated: (ProcessingBlock) -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var editBlock by remember(nodeId) { mutableStateOf(block) }

    // Propagate edits
    fun update(newBlock: ProcessingBlock) {
        editBlock = newBlock
        onBlockUpdated(newBlock)
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp)
                .padding(bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            // -- Header --
            NodeSheetHeader(editBlock, lang)

            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.3f))

            when (editBlock.blockType) {
                BlockType.INPUT_ADAPTER -> InputNodeBody(editBlock, lang, presetType, ::update)
                else -> ProcessNodeBody(editBlock, lang, ::update)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sheet header
// ---------------------------------------------------------------------------

@Composable
private fun NodeSheetHeader(block: ProcessingBlock, lang: String) {
    val accentColor = when (block.blockType) {
        BlockType.INPUT_ADAPTER -> Color(0xFF26A69A)
        BlockType.IMAGE -> Color(0xFFFFA726)
        BlockType.TEXT -> Color(0xFF42A5F5)
        BlockType.AUDIO -> Color(0xFFAB47BC)
    }
    val typeLabel = when (block.blockType) {
        BlockType.INPUT_ADAPTER -> l(lang, "Input Node", "Nút đầu vào", "입력 노드")
        BlockType.IMAGE -> l(lang, "Special Node (Image)", "Nút đặc biệt (Ảnh)", "특수 노드 (이미지)")
        BlockType.TEXT -> l(lang, "Process Node", "Nút xử lý", "처리 노드")
        BlockType.AUDIO -> l(lang, "Special Node (Audio)", "Nút đặc biệt (Âm thanh)", "특수 노드 (오디오)")
    }

    Row(verticalAlignment = Alignment.CenterVertically) {
        Surface(
            modifier = Modifier.size(8.dp),
            shape = CircleShape,
            color = accentColor,
            content = {},
        )
        Spacer(Modifier.width(10.dp))
        Text(
            typeLabel,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
            color = MaterialTheme.colorScheme.onSurface,
        )
    }
}

// ---------------------------------------------------------------------------
// Input node body
// ---------------------------------------------------------------------------

@Composable
internal fun InputNodeBody(
    block: ProcessingBlock,
    lang: String,
    presetType: dev.screengoated.toolbox.mobile.shared.preset.PresetType,
    onUpdate: (ProcessingBlock) -> Unit,
) {
    val isTextInput = presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT ||
        presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT
    val isImage = presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE
    val isAudio = presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC ||
        presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO

    // Show overlay toggle
    SheetSwitchRow(
        icon = Icons.Rounded.RemoveRedEye,
        label = l(lang, "Show overlay", "Hiện overlay", "오버레이 표시"),
        checked = block.showOverlay,
        onCheckedChange = { onUpdate(block.copy(showOverlay = it)) },
    )

    // Render mode (only when overlay visible)
    AnimatedVisibility(
        visible = block.showOverlay,
        enter = fadeIn() + expandVertically(),
        exit = fadeOut() + shrinkVertically(),
    ) {
        InputRenderModeSelector(block, lang, onUpdate)
    }

    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.3f))

    // Auto-copy: text input = locked ON; image = toggleable; audio = hidden
    if (!isAudio) {
        SheetSwitchRow(
            icon = Icons.Rounded.ContentCopy,
            label = if (isTextInput) {
                l(lang, "Auto-copy (always on)", "Tự sao chép (luôn bật)", "자동 복사 (항상 켜짐)")
            } else {
                l(lang, "Auto-copy", "Tự sao chép", "자동 복사")
            },
            checked = if (isTextInput) true else block.autoCopy,
            onCheckedChange = {
                if (!isTextInput) onUpdate(block.copy(autoCopy = it))
                // Text input: locked on, ignore toggle
            },
            enabled = !isTextInput,
        )
    }

    // Auto-speak: only for text input presets
    if (isTextInput) {
        SheetSwitchRow(
            icon = Icons.Rounded.VolumeUp,
            label = l(lang, "Auto-speak", "Tự phát âm", "자동 말하기"),
            checked = block.autoSpeak,
            onCheckedChange = { onUpdate(block.copy(autoSpeak = it)) },
        )
    }
}

@Composable
internal fun InputRenderModeSelector(
    block: ProcessingBlock,
    lang: String,
    onUpdate: (ProcessingBlock) -> Unit,
) {
    val options = listOf(
        RenderModeOption(l(lang, "Normal", "Thường", "일반"), "plain", false),
        RenderModeOption(l(lang, "Markdown", "Đẹp", "마크다운"), "markdown", false),
    )
    val currentIdx = if (block.renderMode == "markdown" || block.renderMode == "markdown_stream") 1 else 0

    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SheetLabel(l(lang, "Render mode", "Chế độ hiển thị", "렌더링 모드"))
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
        ) {
            options.forEachIndexed { idx, opt ->
                ToggleButton(
                    checked = currentIdx == idx,
                    onCheckedChange = {
                        if (it) onUpdate(block.copy(renderMode = opt.renderMode, streamingEnabled = opt.streaming))
                    },
                    shapes = when (idx) {
                        0 -> ButtonGroupDefaults.connectedLeadingButtonShapes()
                        else -> ButtonGroupDefaults.connectedTrailingButtonShapes()
                    },
                    modifier = Modifier.weight(1f).semantics { role = Role.RadioButton },
                ) {
                    Text(opt.label, style = MaterialTheme.typography.labelMedium)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Process / Special node body
// ---------------------------------------------------------------------------

@Composable
internal fun ProcessNodeBody(
    block: ProcessingBlock,
    lang: String,
    onUpdate: (ProcessingBlock) -> Unit,
) {
    // --- Model selector ---
    ModelSelectorSection(block, lang, onUpdate)

    // --- Prompt editor (hidden for non-LLM models) ---
    if (!ModelCatalog.isNonLlm(block.model)) {
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.3f))
        PromptEditorSection(block, lang, onUpdate)
    }

    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.3f))

    // --- Display settings ---
    SheetLabel(l(lang, "Display", "Hiển thị", "표시"))

    SheetSwitchRow(
        icon = Icons.Rounded.RemoveRedEye,
        label = l(lang, "Show overlay", "Hiện overlay", "오버레이 표시"),
        checked = block.showOverlay,
        onCheckedChange = { onUpdate(block.copy(showOverlay = it)) },
    )

    // Render mode (4 options for process/special nodes)
    AnimatedVisibility(
        visible = block.showOverlay,
        enter = fadeIn() + expandVertically(),
        exit = fadeOut() + shrinkVertically(),
    ) {
        RenderModeSelector(block, lang, onUpdate)
    }

    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.3f))

    // --- Auto behaviors ---
    SheetLabel(l(lang, "Auto behaviors", "Hành vi tự động", "자동 동작"))

    SheetSwitchRow(
        icon = Icons.Rounded.ContentCopy,
        label = l(lang, "Auto-copy", "Tự sao chép", "자동 복사"),
        checked = block.autoCopy,
        onCheckedChange = { onUpdate(block.copy(autoCopy = it)) },
    )

    SheetSwitchRow(
        icon = Icons.Rounded.VolumeUp,
        label = l(lang, "Auto-speak", "Tự phát âm", "자동 말하기"),
        checked = block.autoSpeak,
        onCheckedChange = { onUpdate(block.copy(autoSpeak = it)) },
    )
}

// ---------------------------------------------------------------------------
// Model selector
// ---------------------------------------------------------------------------

@Composable
internal fun ModelSelectorSection(
    block: ProcessingBlock,
    lang: String,
    onUpdate: (ProcessingBlock) -> Unit,
) {
    var showPicker by remember { mutableStateOf(false) }
    var searchQuery by remember { mutableStateOf("") }

    val availableModels = remember(block.blockType) {
        ModelCatalog.forBlockType(block.blockType)
    }

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SheetLabel(l(lang, "Model", "Mô hình", "모델"))

        // Current model chip
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(12.dp))
                .clickable { showPicker = !showPicker },
            color = MaterialTheme.colorScheme.surfaceContainerHigh,
            shape = RoundedCornerShape(12.dp),
        ) {
            Row(
                modifier = Modifier.padding(horizontal = 14.dp, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    ModelCatalog.displayName(block.model),
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Medium,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.weight(1f),
                )
                Icon(
                    if (showPicker) Icons.Rounded.Close else Icons.Rounded.Search,
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }

        // Expandable model list
        AnimatedVisibility(
            visible = showPicker,
            enter = fadeIn() + expandVertically(),
            exit = fadeOut() + shrinkVertically(),
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                OutlinedTextField(
                    value = searchQuery,
                    onValueChange = { searchQuery = it },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    placeholder = { Text(l(lang, "Search models...", "Tìm model...", "모델 검색...")) },
                    leadingIcon = {
                        Icon(Icons.Rounded.Search, contentDescription = null, modifier = Modifier.size(18.dp))
                    },
                )

                val filtered = remember(searchQuery, availableModels) {
                    if (searchQuery.isBlank()) availableModels
                    else availableModels.filter {
                        it.displayName.contains(searchQuery, ignoreCase = true) ||
                            it.provider.displayName.contains(searchQuery, ignoreCase = true) ||
                            it.id.contains(searchQuery, ignoreCase = true)
                    }
                }

                val grouped = remember(filtered) { filtered.groupBy { it.provider } }

                LazyColumn(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(max = 240.dp),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    grouped.forEach { (provider, models) ->
                        item(key = "header_${provider.name}") {
                            Text(
                                "${provider.icon} ${provider.displayName}",
                                style = MaterialTheme.typography.labelSmall,
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.padding(top = 8.dp, bottom = 4.dp, start = 4.dp),
                            )
                        }
                        items(models, key = { it.id }) { model ->
                            val isSelected = model.id == block.model
                            Surface(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clip(RoundedCornerShape(8.dp))
                                    .clickable {
                                        onUpdate(block.copy(model = model.id))
                                        showPicker = false
                                        searchQuery = ""
                                    },
                                color = if (isSelected) {
                                    MaterialTheme.colorScheme.primaryContainer
                                } else {
                                    Color.Transparent
                                },
                                shape = RoundedCornerShape(8.dp),
                            ) {
                                Row(
                                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Text(
                                        model.displayName,
                                        style = MaterialTheme.typography.bodySmall,
                                        fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal,
                                        color = if (isSelected) {
                                            MaterialTheme.colorScheme.onPrimaryContainer
                                        } else {
                                            MaterialTheme.colorScheme.onSurface
                                        },
                                    )
                                    if (model.supportsSearch) {
                                        Spacer(Modifier.width(6.dp))
                                        Text(
                                            "\uD83D\uDD0D",
                                            style = MaterialTheme.typography.labelSmall,
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Prompt editor + language variables
// ---------------------------------------------------------------------------

@Composable
internal fun PromptEditorSection(
    block: ProcessingBlock,
    lang: String,
    onUpdate: (ProcessingBlock) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            SheetLabel(l(lang, "Prompt", "Lệnh", "프롬프트"))
            Spacer(Modifier.weight(1f))
            TextButton(
                onClick = {
                    val nextIdx = (block.languageVars.size + 1)
                    val tag = "{language$nextIdx}"
                    val newPrompt = if (block.prompt.endsWith(" ") || block.prompt.isEmpty()) {
                        block.prompt + tag
                    } else {
                        block.prompt + " " + tag
                    }
                    val newVars = block.languageVars.toMutableMap()
                    newVars["language$nextIdx"] = "English"
                    onUpdate(block.copy(prompt = newPrompt, languageVars = newVars))
                },
            ) {
                Icon(Icons.Rounded.Add, contentDescription = null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(4.dp))
                Text(
                    l(lang, "+ Language", "+ Ngôn ngữ", "+ 언어"),
                    style = MaterialTheme.typography.labelMedium,
                )
            }
        }

        OutlinedTextField(
            value = block.prompt,
            onValueChange = { onUpdate(block.copy(prompt = it)) },
            modifier = Modifier.fillMaxWidth(),
            minLines = 2,
            maxLines = 5,
            placeholder = {
                Text(l(lang, "Enter prompt...", "Nhập lệnh...", "프롬프트 입력..."))
            },
        )

        // Language variables
        if (block.languageVars.isNotEmpty()) {
            LanguageVariablesSection(block, lang, onUpdate)
        }
    }
}

@Composable
internal fun LanguageVariablesSection(
    block: ProcessingBlock,
    lang: String,
    onUpdate: (ProcessingBlock) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(
                Icons.Rounded.Language,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.tertiary,
            )
            Spacer(Modifier.width(6.dp))
            SheetLabel(l(lang, "Language variables", "Biến ngôn ngữ", "언어 변수"))
        }

        block.languageVars.entries.sortedBy { it.key }.forEach { (varName, varValue) ->
            LanguageVariableRow(
                varName = varName,
                varValue = varValue,
                lang = lang,
                onValueChanged = { newValue ->
                    val newVars = block.languageVars.toMutableMap()
                    newVars[varName] = newValue
                    onUpdate(block.copy(languageVars = newVars))
                },
                onRemove = {
                    val newVars = block.languageVars.toMutableMap()
                    newVars.remove(varName)
                    // Also remove the tag from prompt
                    val newPrompt = block.prompt.replace("{$varName}", "").trim()
                    onUpdate(block.copy(languageVars = newVars, prompt = newPrompt))
                },
            )
        }
    }
}

@Composable
internal fun LanguageVariableRow(
    varName: String,
    varValue: String,
    lang: String,
    onValueChanged: (String) -> Unit,
    onRemove: () -> Unit,
) {
    var showPicker by remember { mutableStateOf(false) }
    var searchQuery by remember { mutableStateOf("") }

    Column {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                "{$varName}",
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.tertiary,
            )
            Spacer(Modifier.width(8.dp))

            Surface(
                modifier = Modifier
                    .weight(1f)
                    .clip(RoundedCornerShape(8.dp))
                    .clickable { showPicker = !showPicker },
                color = MaterialTheme.colorScheme.tertiaryContainer.copy(alpha = 0.3f),
                shape = RoundedCornerShape(8.dp),
            ) {
                Text(
                    varValue,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                    style = MaterialTheme.typography.bodySmall,
                    fontWeight = FontWeight.Medium,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }

            IconButton(onClick = onRemove, modifier = Modifier.size(32.dp)) {
                Icon(
                    Icons.Rounded.Close,
                    contentDescription = l(lang, "Remove", "Xóa", "삭제"),
                    modifier = Modifier.size(16.dp),
                    tint = MaterialTheme.colorScheme.error,
                )
            }
        }

        AnimatedVisibility(
            visible = showPicker,
            enter = fadeIn() + expandVertically(),
            exit = fadeOut() + shrinkVertically(),
        ) {
            Column(
                modifier = Modifier.padding(top = 6.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                OutlinedTextField(
                    value = searchQuery,
                    onValueChange = { searchQuery = it },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                    placeholder = { Text(l(lang, "Search languages...", "Tìm ngôn ngữ...", "언어 검색...")) },
                    leadingIcon = {
                        Icon(Icons.Rounded.Search, contentDescription = null, modifier = Modifier.size(16.dp))
                    },
                )

                val filtered = remember(searchQuery) {
                    if (searchQuery.isBlank()) LanguageCatalog.names
                    else LanguageCatalog.names.filter {
                        it.contains(searchQuery, ignoreCase = true)
                    }
                }

                LazyColumn(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(max = 200.dp),
                ) {
                    items(filtered, key = { it }) { name ->
                        val isSelected = name == varValue
                        Surface(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(6.dp))
                                .clickable {
                                    onValueChanged(name)
                                    showPicker = false
                                    searchQuery = ""
                                },
                            color = if (isSelected) {
                                MaterialTheme.colorScheme.tertiaryContainer
                            } else {
                                Color.Transparent
                            },
                            shape = RoundedCornerShape(6.dp),
                        ) {
                            Text(
                                name,
                                modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                                style = MaterialTheme.typography.bodySmall,
                                fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Render mode selector (4 options)
// ---------------------------------------------------------------------------

@Composable
internal fun RenderModeSelector(
    block: ProcessingBlock,
    lang: String,
    onUpdate: (ProcessingBlock) -> Unit,
) {
    val options = renderModeOptions(lang)
    val currentIdx = currentRenderModeIndex(block)

    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SheetLabel(l(lang, "Render mode", "Chế độ hiển thị", "렌더링 모드"))
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
        ) {
            options.forEachIndexed { idx, opt ->
                ToggleButton(
                    checked = currentIdx == idx,
                    onCheckedChange = {
                        if (it) onUpdate(
                            block.copy(
                                renderMode = opt.renderMode,
                                streamingEnabled = opt.streaming,
                            ),
                        )
                    },
                    shapes = when (idx) {
                        0 -> ButtonGroupDefaults.connectedLeadingButtonShapes()
                        options.lastIndex -> ButtonGroupDefaults.connectedTrailingButtonShapes()
                        else -> ButtonGroupDefaults.connectedMiddleButtonShapes()
                    },
                    modifier = Modifier.weight(1f).semantics { role = Role.RadioButton },
                ) {
                    Text(opt.label, style = MaterialTheme.typography.labelSmall)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reusable components
// ---------------------------------------------------------------------------

@Composable
internal fun SheetLabel(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleSmall,
        fontWeight = FontWeight.Bold,
        color = MaterialTheme.colorScheme.onSurface,
    )
}

@Composable
internal fun SheetSwitchRow(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    label: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    enabled: Boolean = true,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            icon,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = if (checked) MaterialTheme.colorScheme.primary
            else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = if (enabled) 1f else 0.4f),
        )
        Spacer(Modifier.width(10.dp))
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = if (enabled) 1f else 0.5f),
            modifier = Modifier.weight(1f),
        )
        Switch(checked = checked, onCheckedChange = onCheckedChange, enabled = enabled)
    }
}

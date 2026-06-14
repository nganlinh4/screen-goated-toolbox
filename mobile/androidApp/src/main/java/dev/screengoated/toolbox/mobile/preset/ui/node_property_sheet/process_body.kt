@file:OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.ToggleButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock

// ---------------------------------------------------------------------------
// Render mode model helpers (4-option process/special nodes)
// ---------------------------------------------------------------------------

private fun renderModeOptions(lang: String): List<RenderModeOption> = listOf(
    RenderModeOption(nodeGraphLocalized(lang, "Normal", "Thường", "일반"), "plain", false),
    RenderModeOption(nodeGraphLocalized(lang, "Stream", "Luồng", "스트림"), "stream", true),
    RenderModeOption(nodeGraphLocalized(lang, "Markdown", "Đẹp", "마크다운"), "markdown", false),
    RenderModeOption(nodeGraphLocalized(lang, "MD+Stream", "Đẹp+Luồng", "마크+스트림"), "markdown_stream", true),
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
// Process / Special node body
// ---------------------------------------------------------------------------

@Composable
internal fun ProcessNodeBody(
    block: ProcessingBlock,
    lang: String,
    onUpdate: (ProcessingBlock) -> Unit,
) {
    val isGtx = ModelCatalog.getById(block.model)?.provider == ModelProvider.GOOGLE_GTX

    // --- Model selector ---
    ModelSelectorSection(block, lang, onUpdate)

    // --- Prompt editor (hidden for non-LLM models) ---
    if (!ModelCatalog.isNonLlm(block.model)) {
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.3f))
        PromptEditorSection(block, lang, onUpdate)
    } else if (isGtx) {
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.3f))
        GtxTargetLanguageSection(block, lang, onUpdate)
    }

    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.3f))

    // --- Display settings ---
    SheetLabel(nodeGraphLocalized(lang, "Display", "Hiển thị", "표시"))

    SheetSwitchRow(
        icon = R.drawable.ms_visibility,
        label = nodeGraphLocalized(lang, "Show overlay", "Hiện overlay", "오버레이 표시"),
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
    SheetLabel(nodeGraphLocalized(lang, "Auto behaviors", "Hành vi tự động", "자동 동작"))

    SheetSwitchRow(
        icon = R.drawable.ms_content_copy,
        label = nodeGraphLocalized(lang, "Auto-copy", "Tự sao chép", "자동 복사"),
        checked = block.autoCopy,
        onCheckedChange = { onUpdate(block.copy(autoCopy = it)) },
    )

    SheetSwitchRow(
        icon = R.drawable.ms_volume_up,
        label = nodeGraphLocalized(lang, "Auto-speak", "Tự phát âm", "자동 말하기"),
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
        SheetLabel(nodeGraphLocalized(lang, "Model", "Mô hình", "모델"))

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
                    painterResource(if (showPicker) R.drawable.ms_close else R.drawable.ms_search),
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
                    placeholder = { Text(nodeGraphLocalized(lang, "Search models...", "Tìm model...", "모델 검색...")) },
                    leadingIcon = {
                        Icon(painterResource(R.drawable.ms_search), contentDescription = null, modifier = Modifier.size(18.dp))
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
                                            "🔍",
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
// Prompt editor
// ---------------------------------------------------------------------------

@Composable
internal fun PromptEditorSection(
    block: ProcessingBlock,
    lang: String,
    onUpdate: (ProcessingBlock) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            SheetLabel(nodeGraphLocalized(lang, "Prompt", "Lệnh", "프롬프트"))
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
                Icon(painterResource(R.drawable.ms_add), contentDescription = null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(4.dp))
                Text(
                    nodeGraphLocalized(lang, "+ Language", "+ Ngôn ngữ", "+ 언어"),
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
                Text(nodeGraphLocalized(lang, "Enter prompt...", "Nhập lệnh...", "프롬프트 입력..."))
            },
        )

        // Language variables
        if (block.languageVars.isNotEmpty()) {
            LanguageVariablesSection(block, lang, onUpdate)
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
        SheetLabel(nodeGraphLocalized(lang, "Render mode", "Chế độ hiển thị", "렌더링 모드"))
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

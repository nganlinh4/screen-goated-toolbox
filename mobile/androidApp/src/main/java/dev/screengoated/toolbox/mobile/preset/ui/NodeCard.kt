@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, androidx.compose.ui.text.ExperimentalTextApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

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
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock
import dev.screengoated.toolbox.mobile.ui.ModelPerformancePrefix

// Node colors use Material 3 dynamic accent (Material You) — 3 tonal variants.
private data class NodeColors(
    val bg: Color,
    val title: Color,
    val content: Color,
    val pill: Color,
)

// ---------------------------------------------------------------------------
// Node card composable
// ---------------------------------------------------------------------------

@Composable
internal fun NodeCard(
    node: NodePosition,
    isSelected: Boolean,
    onTap: () -> Unit,
    onDrag: (dx: Float, dy: Float) -> Unit,
    onDragEnd: () -> Unit,
    onDelete: () -> Unit,
    onOutputPinDragStart: () -> Unit,
    onOutputPinDrag: (Offset) -> Unit,
    onOutputPinDragEnd: () -> Unit,
    onMeasured: (heightPx: Float) -> Unit,
    modifier: Modifier = Modifier,
    onBlockUpdated: (ProcessingBlock) -> Unit = {},
    onPromptEditRequest: () -> Unit = {},
    presetType: dev.screengoated.toolbox.mobile.shared.preset.PresetType =
        dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT,
    providerSettings: dev.screengoated.toolbox.mobile.preset.PresetProviderSettings =
        dev.screengoated.toolbox.mobile.preset.PresetProviderSettings(),
    lang: String = "en",
) {
    val block = node.block
    val colors = MaterialTheme.colorScheme

    // 3 accent-derived node styles via Material You dynamic color
    // secondary = lightest/most muted (common Text->Text nodes)
    // tertiary = mid tone (input adapter)
    // primary = boldest (rare special nodes)
    val (cardBg, titleCol, contentCol, pillBg) = when (block.blockType) {
        BlockType.INPUT_ADAPTER -> NodeColors(
            colors.tertiaryContainer, colors.onTertiaryContainer,
            colors.onTertiaryContainer.copy(alpha = 0.75f),
            colors.onTertiaryContainer.copy(alpha = 0.1f),
        )
        BlockType.TEXT -> NodeColors(
            colors.secondaryContainer, colors.onSecondaryContainer,
            colors.onSecondaryContainer.copy(alpha = 0.75f),
            colors.onSecondaryContainer.copy(alpha = 0.1f),
        )
        BlockType.IMAGE, BlockType.AUDIO -> NodeColors(
            colors.primaryContainer, colors.onPrimaryContainer,
            colors.onPrimaryContainer.copy(alpha = 0.75f),
            colors.onPrimaryContainer.copy(alpha = 0.1f),
        )
    }

    Card(
        modifier = modifier
            .width(NODE_WIDTH_DP)
            .onGloballyPositioned { coords -> onMeasured(coords.size.height.toFloat()) }
            .pointerInput(node.id) {
                detectTapGestures(
                    onTap = { onTap() },
                )
            }
            .pointerInput(node.id + "_drag") {
                detectDragGestures(
                    onDrag = { change, dragAmount ->
                        change.consume()
                        onDrag(dragAmount.x, dragAmount.y)
                    },
                    onDragEnd = { onDragEnd() },
                    onDragCancel = { onDragEnd() },
                )
            },
        colors = CardDefaults.cardColors(
            containerColor = cardBg,
        ),
        border = androidx.compose.foundation.BorderStroke(
            0.5.dp,
            titleCol.copy(alpha = 0.15f),
        ),
        shape = MaterialTheme.shapes.medium,
        elevation = CardDefaults.cardElevation(defaultElevation = 1.dp),
    ) {
        Column {
            Row(
                modifier = Modifier.padding(start = 10.dp, end = 10.dp, top = 8.dp),
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

                // Title
                Text(
                    text = nodeTypeLabel(block.blockType, lang, presetType),
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = titleCol,
                    modifier = Modifier.weight(1f),
                )

                // Delete button (not on input adapter)
                if (block.blockType != BlockType.INPUT_ADAPTER) {
                    Box(
                        modifier = Modifier
                            .size(20.dp)
                            .pointerInput(node.id + "_del") {
                                detectTapGestures { onDelete() }
                            },
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(
                            painter = painterResource(R.drawable.ms_close),
                            contentDescription = null,
                            modifier = Modifier.size(12.dp),
                            tint = contentCol.copy(alpha = 0.5f),
                        )
                    }
                }

                // Output pin
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

            // Always-visible inline editor (like Windows)
            Column(
                modifier = Modifier.padding(horizontal = 10.dp).padding(bottom = 8.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                if (block.blockType != BlockType.INPUT_ADAPTER) {
                    // Row 1: "Mô hình:" label + model dropdown (same row)
                    var showModelDropdown by remember { mutableStateOf(false) }
                    val catalog = dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
                    val descriptor = catalog.getById(block.model)
                    val isNonLlm = descriptor?.isNonLlm == true
                    val isGtx = descriptor?.provider == PresetModelProvider.GOOGLE_GTX
                    val availableModels = remember(block.blockType, providerSettings) {
                        catalog.forBlockType(block.blockType).filter { model ->
                            when (model.provider) {
                                dev.screengoated.toolbox.mobile.preset.PresetModelProvider.GROQ -> providerSettings.useGroq
                                dev.screengoated.toolbox.mobile.preset.PresetModelProvider.GOOGLE,
                                dev.screengoated.toolbox.mobile.preset.PresetModelProvider.GEMINI_LIVE,
                                -> providerSettings.useGemini
                                dev.screengoated.toolbox.mobile.preset.PresetModelProvider.OPENROUTER -> providerSettings.useOpenRouter
                                dev.screengoated.toolbox.mobile.preset.PresetModelProvider.CEREBRAS -> providerSettings.useCerebras
                                dev.screengoated.toolbox.mobile.preset.PresetModelProvider.OLLAMA -> providerSettings.useOllama
                                else -> true
                            }
                        }
                    }

                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            text = nodeGraphModelLabel(lang),
                            style = MaterialTheme.typography.labelSmall,
                            color = contentCol.copy(alpha = 0.6f),
                        )
                        Spacer(Modifier.width(4.dp))
                        Box {
                            Surface(
                                modifier = Modifier
                                    .pointerInput(Unit) { detectTapGestures { showModelDropdown = true } },
                                shape = RoundedCornerShape(4.dp),
                                color = pillBg,
                            ) {
                                Text(
                                    text = descriptor?.localizedName(lang) ?: block.model,
                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                                    style = MaterialTheme.typography.labelSmall,
                                    color = contentCol,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                            androidx.compose.material3.DropdownMenu(
                                expanded = showModelDropdown,
                                onDismissRequest = { showModelDropdown = false },
                                modifier = Modifier.widthIn(min = 300.dp),
                            ) {
                                availableModels.forEach { model ->
                                    val providerIcon = providerIconRes(model.provider)
                                    val hasSearch = catalog.supportsSearchById(model.id)
                                    val isSelected = model.id == block.model
                                    androidx.compose.material3.DropdownMenuItem(
                                        modifier = if (isSelected) Modifier
                                            .padding(horizontal = 4.dp)
                                            .background(
                                                MaterialTheme.colorScheme.primary.copy(alpha = 0.08f),
                                                RoundedCornerShape(8.dp),
                                            )
                                        else Modifier,
                                        leadingIcon = {
                                            Row(verticalAlignment = Alignment.CenterVertically) {
                                                ModelPerformancePrefix(model)
                                                Spacer(Modifier.width(6.dp))
                                                Icon(
                                                    painterResource(providerIcon),
                                                    null,
                                                    modifier = Modifier.size(16.dp),
                                                )
                                            }
                                        },
                                        trailingIcon = {
                                            if (hasSearch) {
                                                Icon(
                                                    painterResource(R.drawable.ms_search),
                                                    null,
                                                    modifier = Modifier.size(14.dp),
                                                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                                                )
                                            }
                                        },
                                        text = {
                                            val quota = model.localizedQuota(lang)
                                            val suffix = if (quota.isNotBlank()) " - ${model.fullName} - $quota"
                                                else " - ${model.fullName}"
                                            Text(
                                                text = androidx.compose.ui.text.buildAnnotatedString {
                                                    pushStyle(androidx.compose.ui.text.SpanStyle(
                                                        fontWeight = if (isSelected) FontWeight.Bold else FontWeight.SemiBold,
                                                    ))
                                                    append(model.localizedName(lang))
                                                    pop()
                                                    pushStyle(androidx.compose.ui.text.SpanStyle(
                                                        fontSize = 11.sp,
                                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                                        fontFamily = condensedFontFamily,
                                                    ))
                                                    append(suffix)
                                                    pop()
                                                },
                                                style = MaterialTheme.typography.bodySmall,
                                            )
                                        },
                                        onClick = {
                                            onBlockUpdated(block.copy(model = model.id))
                                            showModelDropdown = false
                                        },
                                    )
                                }
                            }
                        }
                    }

                    // Row 2: "Lệnh:" label + "+ Ngôn ngữ" button (only for LLM models)
                    if (!isNonLlm) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                text = nodeGraphPromptLabel(lang),
                                style = MaterialTheme.typography.labelSmall,
                                color = contentCol.copy(alpha = 0.6f),
                            )
                            Spacer(Modifier.weight(1f))
                            // "+ Ngôn ngữ" button
                            Surface(
                                modifier = Modifier
                                    .pointerInput(node.id + "_addlang") {
                                        detectTapGestures {
                                            // Find next available language slot (max 10)
                                            val existing = block.languageVars.keys
                                                .mapNotNull { it.removePrefix("language").toIntOrNull() }
                                                .toSet()
                                            val nextN = (1..10).firstOrNull { it !in existing } ?: return@detectTapGestures
                                            val newKey = "language$nextN"
                                            val newPrompt = block.prompt + " {$newKey}"
                                            val newVars = block.languageVars + (newKey to "Vietnamese")
                                            onBlockUpdated(block.copy(prompt = newPrompt, languageVars = newVars))
                                        }
                                    },
                                shape = RoundedCornerShape(8.dp),
                                color = Color(0xFF5A8A90).copy(alpha = 0.8f),
                            ) {
                                Text(
                                    text = nodeGraphAddLanguageLabel(lang),
                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp),
                                    style = MaterialTheme.typography.labelSmall,
                                    color = Color.White,
                                )
                            }
                        }
                    }

                    // Row 3: Prompt text preview
                    if (!isNonLlm) {
                    Surface(
                        modifier = Modifier
                            .fillMaxWidth()
                            .pointerInput(node.id + "_prompt") {
                                detectTapGestures { onPromptEditRequest() }
                            },
                        shape = RoundedCornerShape(6.dp),
                        color = pillBg,
                    ) {
                        Text(
                            text = block.prompt.ifBlank { nodeGraphPromptPlaceholder(lang) },
                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 6.dp),
                            style = MaterialTheme.typography.bodySmall,
                            color = if (block.prompt.isBlank())
                                contentCol.copy(alpha = 0.4f)
                            else contentCol,
                            maxLines = 4,
                            overflow = TextOverflow.Ellipsis,
                            lineHeight = 14.sp,
                        )
                    }
                    } // end if (!isNonLlm) for prompt

                    // Row 4+: Language variable rows. GTX is non-LLM but still needs language1.
                    if (!isNonLlm || isGtx) {
                        val detectedVars = if (isGtx) {
                            listOf(1)
                        } else {
                            (1..10).filter { n ->
                                block.prompt.contains("{language$n}")
                            }
                        }
                        detectedVars.forEach { num ->
                            val key = "language$num"
                            // Auto-insert default if tag exists but no map entry
                            val currentValue = block.languageVars[key] ?: run {
                                val newVars = block.languageVars + (key to "Vietnamese")
                                onBlockUpdated(block.copy(languageVars = newVars))
                                "Vietnamese"
                            }
                            var showLangDropdown by remember { mutableStateOf(false) }
                            var langSearchQuery by remember { mutableStateOf("") }
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    text = "{$key}:",
                                    style = MaterialTheme.typography.labelSmall,
                                    color = contentCol.copy(alpha = 0.5f),
                                )
                                Spacer(Modifier.width(4.dp))
                                Box {
                                    Surface(
                                        modifier = Modifier
                                            .pointerInput(key) { detectTapGestures { showLangDropdown = true } },
                                        shape = RoundedCornerShape(8.dp),
                                        color = Color(0xFF6E5AAF).copy(alpha = 0.25f),
                                    ) {
                                        Text(
                                            text = currentValue,
                                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp),
                                            style = MaterialTheme.typography.labelSmall,
                                            fontWeight = FontWeight.SemiBold,
                                            color = contentCol,
                                        )
                                    }
                                    androidx.compose.material3.DropdownMenu(
                                        expanded = showLangDropdown,
                                        onDismissRequest = {
                                            showLangDropdown = false
                                            langSearchQuery = ""
                                        },
                                        modifier = Modifier.widthIn(min = 200.dp),
                                        properties = androidx.compose.ui.window.PopupProperties(focusable = true),
                                    ) {
                                        // Sticky search box
                                        androidx.compose.material3.OutlinedTextField(
                                            value = langSearchQuery,
                                            onValueChange = { langSearchQuery = it },
                                            modifier = Modifier
                                                .fillMaxWidth()
                                                .padding(horizontal = 8.dp, vertical = 4.dp),
                                            placeholder = { Text(nodeGraphLanguageSearchPlaceholder(lang), style = MaterialTheme.typography.bodySmall) },
                                            singleLine = true,
                                            textStyle = MaterialTheme.typography.bodySmall,
                                        )
                                        androidx.compose.material3.HorizontalDivider()
                                        // Scrollable language list
                                        val filteredLangs = remember(langSearchQuery) {
                                            val query = langSearchQuery.lowercase()
                                            ALL_ISO_LANGUAGES.filter {
                                                query.isEmpty() || it.lowercase().contains(query)
                                            }
                                        }
                                        Column(
                                            modifier = Modifier
                                                .heightIn(max = 250.dp)
                                                .verticalScroll(rememberScrollState()),
                                        ) {
                                        filteredLangs.forEach { language ->
                                            androidx.compose.material3.DropdownMenuItem(
                                                text = {
                                                    Text(
                                                        language,
                                                        style = MaterialTheme.typography.bodySmall,
                                                        fontWeight = if (language == currentValue) FontWeight.Bold else FontWeight.Normal,
                                                    )
                                                },
                                                onClick = {
                                                    val newVars = block.languageVars.toMutableMap()
                                                    newVars[key] = language
                                                    onBlockUpdated(block.copy(languageVars = newVars))
                                                    showLangDropdown = false
                                                    langSearchQuery = ""
                                                },
                                            )
                                        }
                                        } // end Column (scrollable)
                                    }
                                }
                            }
                        }
                    }

                    // Bottom icon toolbar row
                    NodeToolbarRow(
                        block = block,
                        lang = lang,
                        contentCol = contentCol,
                        pillBg = pillBg,
                        streamingActive = block.streamingEnabled,
                        onBlockUpdated = onBlockUpdated,
                    )
                } else {
                    // Input node: eye + render mode + copy + speak (like Windows)
                    NodeToolbarRow(
                        block = block,
                        lang = lang,
                        contentCol = contentCol,
                        pillBg = pillBg,
                        streamingActive = block.streamingEnabled || block.renderMode == "markdown_stream",
                        onBlockUpdated = onBlockUpdated,
                    )
                }
            }
        }
    }
}

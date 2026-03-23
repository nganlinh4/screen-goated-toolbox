package dev.screengoated.toolbox.mobile.ui

import android.util.Log
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Computer
import androidx.compose.material.icons.rounded.DragIndicator
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material.icons.rounded.LocalFireDepartment
import androidx.compose.material.icons.rounded.Public
import androidx.compose.material.icons.rounded.RestartAlt
import androidx.compose.material.icons.rounded.Translate
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import dev.screengoated.toolbox.mobile.preset.GeneratedPresetModelCatalogData
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelDescriptor
import dev.screengoated.toolbox.mobile.preset.PresetModelPriorityChains
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.PresetModelType
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlin.math.roundToInt

private const val PRESET_RUNTIME_DRAG_LOG_TAG = "PresetRuntimeDrag"

private fun providerIcon(provider: PresetModelProvider): ImageVector = when (provider) {
    PresetModelProvider.GOOGLE, PresetModelProvider.GEMINI_LIVE -> Icons.Rounded.AutoAwesome
    PresetModelProvider.GOOGLE_GTX -> Icons.Rounded.Translate
    PresetModelProvider.GROQ -> Icons.Rounded.Bolt
    PresetModelProvider.CEREBRAS -> Icons.Rounded.LocalFireDepartment
    PresetModelProvider.OPENROUTER -> Icons.Rounded.Public
    PresetModelProvider.OLLAMA -> Icons.Rounded.Computer
    else -> Icons.Rounded.AutoAwesome
}

@Composable
fun PresetRuntimeSettingsDialog(
    settings: PresetRuntimeSettings,
    locale: MobileLocaleText,
    uiLanguage: String = "en",
    onDismiss: () -> Unit,
    onSave: (PresetRuntimeSettings) -> Unit,
) {
    var imageChain by remember(settings) { mutableStateOf(settings.modelPriorityChains.imageToText.toMutableList()) }
    var textChain by remember(settings) { mutableStateOf(settings.modelPriorityChains.textToText.toMutableList()) }
    var showHelpDialog by remember { mutableStateOf(false) }

    if (showHelpDialog) {
        androidx.compose.material3.AlertDialog(
            onDismissRequest = { showHelpDialog = false },
            title = { Text(locale.presetRuntimeTitle) },
            text = { Text(locale.presetRuntimeDescription) },
            confirmButton = {
                TextButton(onClick = { showHelpDialog = false }) { Text("OK") }
            },
        )
    }

    fun applyChanges(img: List<String> = imageChain, txt: List<String> = textChain) {
        onSave(settings.copy(modelPriorityChains = PresetModelPriorityChains(
            imageToText = img, textToText = txt,
        )))
    }

    val configuration = LocalConfiguration.current
    val isLandscape = configuration.screenWidthDp > configuration.screenHeightDp

    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Card(
            modifier = Modifier
                .widthIn(max = if (isLandscape) 720.dp else 420.dp)
                .padding(16.dp),
            shape = RoundedCornerShape(28.dp),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        ) {
            Column(modifier = Modifier.padding(start = 24.dp, end = 12.dp, top = 12.dp, bottom = 12.dp)) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        locale.presetRuntimeTitle,
                        style = MaterialTheme.typography.titleLarge,
                        modifier = Modifier.weight(1f),
                    )
                    IconButton(onClick = { showHelpDialog = true }) {
                        Icon(Icons.Rounded.Info, contentDescription = locale.presetRuntimeDescription)
                    }
                    IconButton(onClick = onDismiss) {
                        Icon(Icons.Rounded.Close, contentDescription = null)
                    }
                }
                Spacer(Modifier.size(8.dp))

                if (isLandscape) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .weight(1f, fill = false)
                            .heightIn(max = 400.dp),
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        Column(
                            modifier = Modifier
                                .weight(1f)
                                .verticalScroll(rememberScrollState()),
                        ) {
                            ChainEditor(
                                title = locale.presetRuntimeImageChainLabel,
                                chain = imageChain,
                                modelType = PresetModelType.VISION,
                                defaultChain = GeneratedPresetModelCatalogData.modelPriorityChains.imageToText,
                                locale = locale,
                                uiLanguage = uiLanguage,
                                onChainChanged = { imageChain = it.toMutableList(); applyChanges(img = it) },
                            )
                        }
                        Column(
                            modifier = Modifier
                                .weight(1f)
                                .verticalScroll(rememberScrollState()),
                        ) {
                            ChainEditor(
                                title = locale.presetRuntimeTextChainLabel,
                                chain = textChain,
                                modelType = PresetModelType.TEXT,
                                defaultChain = GeneratedPresetModelCatalogData.modelPriorityChains.textToText,
                                locale = locale,
                                uiLanguage = uiLanguage,
                                onChainChanged = { textChain = it.toMutableList(); applyChanges(txt = it) },
                            )
                        }
                    }
                } else {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .weight(1f, fill = false)
                            .verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.spacedBy(16.dp),
                    ) {
                        ChainEditor(
                            title = locale.presetRuntimeImageChainLabel,
                            chain = imageChain,
                            modelType = PresetModelType.VISION,
                            defaultChain = GeneratedPresetModelCatalogData.modelPriorityChains.imageToText,
                            locale = locale,
                            uiLanguage = uiLanguage,
                            onChainChanged = { imageChain = it.toMutableList(); applyChanges(img = it) },
                        )
                        ChainEditor(
                            title = locale.presetRuntimeTextChainLabel,
                            chain = textChain,
                            modelType = PresetModelType.TEXT,
                            defaultChain = GeneratedPresetModelCatalogData.modelPriorityChains.textToText,
                            locale = locale,
                            uiLanguage = uiLanguage,
                            onChainChanged = { textChain = it.toMutableList(); applyChanges(txt = it) },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ChainEditor(
    title: String,
    chain: List<String>,
    modelType: PresetModelType,
    defaultChain: List<String>,
    locale: MobileLocaleText,
    uiLanguage: String,
    onChainChanged: (List<String>) -> Unit,
) {
    val availableModels = remember(modelType) { PresetModelCatalog.forType(modelType) }
    var showAddMenu by remember { mutableStateOf(false) }
    var draggedModelId by remember { mutableStateOf<String?>(null) }
    var dragOffsetY by remember { mutableFloatStateOf(0f) }
    val itemHeightPx = with(LocalDensity.current) { 44.dp.toPx() }
    val latestChain by rememberUpdatedState(chain)
    val latestOnChainChanged by rememberUpdatedState(onChainChanged)

    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            // Header: title + restore default
            Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(title, style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f))
                TextButton(onClick = { onChainChanged(defaultChain) }) {
                    Icon(Icons.Rounded.RestartAlt, null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(4.dp))
                    Text(locale.presetRuntimeRestoreDefault, style = MaterialTheme.typography.labelSmall)
                }
            }

            // Entry 1: Fixed "Chosen model → always first"
            FixedEntryRow(
                number = 1,
                label = locale.presetRuntimeChosenModel,
                hint = locale.presetRuntimeChosenHint,
            )

            // Editable entries (numbered from 2)
            chain.forEachIndexed { index, modelId ->
                key(modelId) {
                    val isDragging = draggedModelId == modelId
                    val elevation by animateDpAsState(if (isDragging) 8.dp else 0.dp, label = "elev")
                    val dragVisualModifier = if (isDragging) {
                        Modifier
                            .offset { IntOffset(0, dragOffsetY.roundToInt()) }
                            .zIndex(10f)
                            .shadow(elevation, RoundedCornerShape(20.dp))
                    } else {
                        Modifier
                    }
                    val dragHandleModifier = Modifier
                        .width(44.dp)
                        .heightIn(min = 32.dp)
                        .pointerInput(modelId) {
                            detectDragGestures(
                                onDragStart = {
                                    Log.d(
                                        PRESET_RUNTIME_DRAG_LOG_TAG,
                                        "drag_start modelId=$modelId chain=${latestChain.joinToString()}",
                                    )
                                    draggedModelId = modelId
                                    dragOffsetY = 0f
                                },
                                onDrag = { change, offset ->
                                    change.consume()
                                    val deltaY = offset.y
                                    val activeId = draggedModelId
                                    if (activeId == null) {
                                        Log.w(
                                            PRESET_RUNTIME_DRAG_LOG_TAG,
                                            "drag_delta_without_active modelId=$modelId deltaY=$deltaY",
                                        )
                                    } else {
                                        val currentChain = latestChain
                                        val currentIndex = currentChain.indexOf(activeId)
                                        if (currentIndex == -1) {
                                            Log.w(
                                                PRESET_RUNTIME_DRAG_LOG_TAG,
                                                "drag_missing_active modelId=$modelId activeId=$activeId chain=${currentChain.joinToString()}",
                                            )
                                            draggedModelId = null
                                            dragOffsetY = 0f
                                        } else {
                                            dragOffsetY += deltaY
                                            val steps = (dragOffsetY / itemHeightPx).roundToInt()
                                            Log.d(
                                                PRESET_RUNTIME_DRAG_LOG_TAG,
                                                "drag_move modelId=$modelId activeId=$activeId currentIndex=$currentIndex deltaY=$deltaY dragOffsetY=$dragOffsetY steps=$steps chain=${currentChain.joinToString()}",
                                            )
                                            if (steps != 0) {
                                                val targetIndex = (currentIndex + steps).coerceIn(0, currentChain.lastIndex)
                                                if (targetIndex != currentIndex) {
                                                    val list = currentChain.toMutableList()
                                                    val item = list.removeAt(currentIndex)
                                                    list.add(targetIndex, item)
                                                    Log.d(
                                                        PRESET_RUNTIME_DRAG_LOG_TAG,
                                                        "drag_reorder activeId=$activeId from=$currentIndex to=$targetIndex updatedChain=${list.joinToString()}",
                                                    )
                                                    latestOnChainChanged(list)
                                                    dragOffsetY -= (targetIndex - currentIndex) * itemHeightPx
                                                }
                                            }
                                        }
                                    }
                                },
                                onDragEnd = {
                                    Log.d(
                                        PRESET_RUNTIME_DRAG_LOG_TAG,
                                        "drag_end draggedModelId=$draggedModelId",
                                    )
                                    draggedModelId = null
                                    dragOffsetY = 0f
                                },
                                onDragCancel = {
                                    Log.d(
                                        PRESET_RUNTIME_DRAG_LOG_TAG,
                                        "drag_cancel draggedModelId=$draggedModelId",
                                    )
                                    draggedModelId = null
                                    dragOffsetY = 0f
                                },
                            )
                        }

                    ModelPill(
                        number = index + 2,
                        modelId = modelId,
                        availableModels = availableModels,
                        uiLanguage = uiLanguage,
                        isDragging = isDragging,
                        modifier = dragVisualModifier,
                        dragHandleModifier = dragHandleModifier,
                        onModelChanged = { newId ->
                            val list = latestChain.toMutableList()
                            val modelIndex = list.indexOf(modelId)
                            if (modelIndex != -1) {
                                list[modelIndex] = newId
                                latestOnChainChanged(list)
                            }
                        },
                        onRemove = {
                            val list = latestChain.toMutableList()
                            val modelIndex = list.indexOf(modelId)
                            if (modelIndex != -1) {
                                list.removeAt(modelIndex)
                                if (draggedModelId == modelId) {
                                    draggedModelId = null
                                    dragOffsetY = 0f
                                }
                                latestOnChainChanged(list)
                            }
                        },
                    )
                }
            }

            // Add model button
            Box {
                FilledTonalButton(
                    onClick = { showAddMenu = true },
                    shape = RoundedCornerShape(20.dp),
                ) {
                    Text(locale.presetRuntimeAddModel, style = MaterialTheme.typography.labelSmall)
                }
                DropdownMenu(expanded = showAddMenu, onDismissRequest = { showAddMenu = false }) {
                    availableModels.filter { it.id !in chain }.forEach { model ->
                        ModelDropdownItem(model, uiLanguage) {
                            onChainChanged(chain + model.id)
                            showAddMenu = false
                        }
                    }
                }
            }

            // Last entry: Fixed "Auto → smart fallback"
            FixedEntryRow(
                number = chain.size + 2,
                label = locale.presetRuntimeAuto,
                hint = locale.presetRuntimeAutoHint,
            )
        }
    }
}

@Composable
private fun FixedEntryRow(number: Int, label: String, hint: String) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp, horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            "$number.",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.width(6.dp))
        Text(label, style = MaterialTheme.typography.bodyMedium, fontWeight = androidx.compose.ui.text.font.FontWeight.Bold)
        Spacer(Modifier.width(6.dp))
        Text("→", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.width(6.dp))
        Text(hint, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun ModelPill(
    number: Int,
    modelId: String,
    availableModels: List<PresetModelDescriptor>,
    uiLanguage: String,
    isDragging: Boolean,
    modifier: Modifier = Modifier,
    dragHandleModifier: Modifier = Modifier,
    onModelChanged: (String) -> Unit,
    onRemove: () -> Unit,
) {
    val descriptor = PresetModelCatalog.getById(modelId)
    var showDropdown by remember { mutableStateOf(false) }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(20.dp))
            .background(
                if (isDragging) MaterialTheme.colorScheme.surfaceContainerHighest
                else MaterialTheme.colorScheme.surfaceContainer,
            )
            .padding(start = 12.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = dragHandleModifier,
            contentAlignment = Alignment.Center,
        ) {
            Log.d(
                PRESET_RUNTIME_DRAG_LOG_TAG,
                "handle_bound modelId=$modelId isDragging=$isDragging",
            )
            Icon(
                Icons.Rounded.DragIndicator,
                contentDescription = null,
                modifier = Modifier.size(20.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Spacer(Modifier.width(4.dp))
        Text(
            "$number.",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.width(6.dp))
        Box(modifier = Modifier.weight(1f)) {
            TextButton(onClick = { showDropdown = true }, modifier = Modifier.fillMaxWidth()) {
                if (descriptor != null) {
                    Icon(providerIcon(descriptor.provider), null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(4.dp))
                }
                Text(
                    descriptor?.localizedName(uiLanguage) ?: modelId,
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 1,
                    modifier = Modifier.weight(1f),
                )
            }
            DropdownMenu(expanded = showDropdown, onDismissRequest = { showDropdown = false }) {
                availableModels.forEach { model ->
                    ModelDropdownItem(model, uiLanguage) {
                        onModelChanged(model.id)
                        showDropdown = false
                    }
                }
            }
        }
        IconButton(onClick = onRemove, modifier = Modifier.size(28.dp)) {
            Icon(Icons.Rounded.Close, null, modifier = Modifier.size(14.dp))
        }
    }
}

@Composable
private fun ModelDropdownItem(model: PresetModelDescriptor, uiLanguage: String, onClick: () -> Unit) {
    DropdownMenuItem(
        leadingIcon = {
            Icon(providerIcon(model.provider), null, modifier = Modifier.size(18.dp))
        },
        text = {
            Column {
                Text(model.localizedName(uiLanguage), style = MaterialTheme.typography.bodySmall)
                Text(
                    model.fullName,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        },
        onClick = onClick,
    )
}

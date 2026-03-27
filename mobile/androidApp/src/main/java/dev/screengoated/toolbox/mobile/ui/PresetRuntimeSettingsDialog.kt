@file:OptIn(androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import android.util.Log
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
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.platform.LocalConfiguration
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
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
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
    PresetModelProvider.TAALAS -> Icons.Rounded.RocketLaunch
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
        ExpressiveDialogSurface(
            title = locale.presetRuntimeTitle,
            icon = Icons.Rounded.Info,
            accent = MaterialTheme.colorScheme.primary,
            morphPair = ExpressiveMorphPair(
                MaterialShapes.Circle,
                MaterialShapes.Cookie4Sided,
            ),
            onDismiss = { showHelpDialog = false },
            fitContentHeight = true,
            maxWidth = 460.dp,
        ) {
            ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.primary) {
                Text(
                    text = locale.presetRuntimeDescription,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        }
    }

    fun applyChanges(img: List<String> = imageChain, txt: List<String> = textChain) {
        onSave(settings.copy(modelPriorityChains = PresetModelPriorityChains(
            imageToText = img, textToText = txt,
        )))
    }

    val configuration = LocalConfiguration.current
    val isLandscape = configuration.screenWidthDp > configuration.screenHeightDp

    ExpressiveDialogSurface(
        title = locale.presetRuntimeTitle,
        icon = Icons.Rounded.Settings,
        accent = MaterialTheme.colorScheme.primary,
        morphPair = ExpressiveMorphPair(
            MaterialShapes.Square,
            MaterialShapes.Cookie6Sided,
        ),
        onDismiss = onDismiss,
        widthFraction = if (isLandscape) 0.92f else 0.96f,
        maxWidth = if (isLandscape) 900.dp else 520.dp,
        heightFraction = 0.88f,
        maxHeight = 760.dp,
        fitContentHeight = true,
        headerTrailing = {
            IconButton(onClick = { showHelpDialog = true }) {
                Icon(Icons.Rounded.Info, contentDescription = locale.presetRuntimeDescription)
            }
        },
    ) {
        if (isLandscape) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = 520.dp),
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
                        accent = MaterialTheme.colorScheme.primary,
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
                        accent = MaterialTheme.colorScheme.secondary,
                        onChainChanged = { textChain = it.toMutableList(); applyChanges(txt = it) },
                    )
                }
            }
        } else {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = 560.dp)
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
                    accent = MaterialTheme.colorScheme.primary,
                    onChainChanged = { imageChain = it.toMutableList(); applyChanges(img = it) },
                )
                ChainEditor(
                    title = locale.presetRuntimeTextChainLabel,
                    chain = textChain,
                    modelType = PresetModelType.TEXT,
                    defaultChain = GeneratedPresetModelCatalogData.modelPriorityChains.textToText,
                    locale = locale,
                    uiLanguage = uiLanguage,
                    accent = MaterialTheme.colorScheme.secondary,
                    onChainChanged = { textChain = it.toMutableList(); applyChanges(txt = it) },
                )
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
    accent: androidx.compose.ui.graphics.Color,
    onChainChanged: (List<String>) -> Unit,
) {
    val availableModels = remember(modelType) { PresetModelCatalog.forType(modelType) }
    var showAddMenu by remember { mutableStateOf(false) }
    var draggedModelId by remember { mutableStateOf<String?>(null) }
    var dragOffsetY by remember { mutableFloatStateOf(0f) }
    val itemHeightPx = with(LocalDensity.current) { 44.dp.toPx() }
    val latestChain by rememberUpdatedState(chain)
    val latestOnChainChanged by rememberUpdatedState(onChainChanged)

    ExpressiveDialogSectionCard(
        accent = accent,
        modifier = Modifier.fillMaxWidth(),
    ) {
        UtilityHeaderRow(
            icon = if (modelType == PresetModelType.VISION) Icons.Rounded.AutoAwesome else Icons.Rounded.Translate,
            title = title,
            accent = accent,
            trailing = {
                TextButton(onClick = { onChainChanged(defaultChain) }) {
                    Icon(Icons.Rounded.RestartAlt, null, modifier = Modifier.size(16.dp), tint = accent)
                    Spacer(Modifier.width(6.dp))
                    Text(locale.presetRuntimeRestoreDefault, color = accent)
                }
            }
        )

        FixedEntryRow(
            number = 1,
            label = locale.presetRuntimeChosenModel,
            hint = locale.presetRuntimeChosenHint,
            accent = accent,
        )

        chain.forEachIndexed { index, modelId ->
            key(modelId) {
                DraggableModelPill(
                    number = index + 2,
                    modelId = modelId,
                    availableModels = availableModels,
                    uiLanguage = uiLanguage,
                    accent = accent,
                    isDragging = draggedModelId == modelId,
                    dragOffsetY = dragOffsetY,
                    onModelChanged = { newId ->
                        val list = latestChain.toMutableList()
                        val modelIndex = list.indexOf(modelId)
                        if (modelIndex != -1) {
                            list[modelIndex] = newId
                            latestOnChainChanged(list)
                        }
                    },
                    onDragStart = {
                        Log.d(PRESET_RUNTIME_DRAG_LOG_TAG, "drag_start modelId=$modelId chain=${latestChain.joinToString()}")
                        draggedModelId = modelId
                        dragOffsetY = 0f
                    },
                    onDragDelta = { deltaY ->
                        val activeId = draggedModelId
                        if (activeId == null) {
                            Log.w(PRESET_RUNTIME_DRAG_LOG_TAG, "drag_delta_without_active modelId=$modelId deltaY=$deltaY")
                        } else {
                            val currentChain = latestChain
                            val currentIndex = currentChain.indexOf(activeId)
                            if (currentIndex == -1) {
                                Log.w(PRESET_RUNTIME_DRAG_LOG_TAG, "drag_missing_active modelId=$modelId activeId=$activeId chain=${currentChain.joinToString()}")
                                draggedModelId = null
                                dragOffsetY = 0f
                            } else {
                                dragOffsetY += deltaY
                                val steps = (dragOffsetY / itemHeightPx).roundToInt()
                                if (steps != 0) {
                                    val targetIndex = (currentIndex + steps).coerceIn(0, currentChain.lastIndex)
                                    if (targetIndex != currentIndex) {
                                        val list = currentChain.toMutableList()
                                        val item = list.removeAt(currentIndex)
                                        list.add(targetIndex, item)
                                        latestOnChainChanged(list)
                                        dragOffsetY -= (targetIndex - currentIndex) * itemHeightPx
                                    }
                                }
                            }
                        }
                    },
                    onDragEnd = {
                        draggedModelId = null
                        dragOffsetY = 0f
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

        Box {
            TextButton(onClick = { showAddMenu = true }) {
                Text(locale.presetRuntimeAddModel, color = accent)
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

        FixedEntryRow(
            number = chain.size + 2,
            label = locale.presetRuntimeAuto,
            hint = locale.presetRuntimeAutoHint,
            accent = accent,
        )
    }
}

@Composable
private fun DraggableModelPill(
    number: Int,
    modelId: String,
    availableModels: List<PresetModelDescriptor>,
    uiLanguage: String,
    accent: androidx.compose.ui.graphics.Color,
    isDragging: Boolean,
    dragOffsetY: Float,
    onDragStart: () -> Unit,
    onDragDelta: (Float) -> Unit,
    onDragEnd: (Boolean) -> Unit,
    onModelChanged: (String) -> Unit,
    onRemove: () -> Unit,
) {
    val dragVisualModifier = if (isDragging) {
        Modifier
            .offset { IntOffset(0, dragOffsetY.roundToInt()) }
            .zIndex(10f)
    } else {
        Modifier
    }
    val currentOnDragStart by rememberUpdatedState(onDragStart)
    val currentOnDragDelta by rememberUpdatedState(onDragDelta)
    val currentOnDragEnd by rememberUpdatedState(onDragEnd)
    val dragHandleModifier = remember(modelId) {
        Modifier
            .width(44.dp)
            .heightIn(min = 32.dp)
            .pointerInput(modelId) {
                detectDragGestures(
                    onDragStart = { currentOnDragStart() },
                    onDrag = { change, offset ->
                        change.consume()
                        currentOnDragDelta(offset.y)
                    },
                    onDragEnd = { currentOnDragEnd(false) },
                    onDragCancel = { currentOnDragEnd(true) },
                )
            }
    }

    ModelPill(
        number = number,
        modelId = modelId,
        availableModels = availableModels,
        uiLanguage = uiLanguage,
        accent = accent,
        isDragging = isDragging,
        modifier = dragVisualModifier,
        dragHandleModifier = dragHandleModifier,
        onModelChanged = onModelChanged,
        onRemove = onRemove,
    )
}

@Composable
private fun FixedEntryRow(
    number: Int,
    label: String,
    hint: String,
    accent: androidx.compose.ui.graphics.Color,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(MaterialTheme.shapes.small)
            .background(accent.copy(alpha = 0.08f))
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
    accent: androidx.compose.ui.graphics.Color,
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
            .clip(MaterialTheme.shapes.small)
            .background(
                if (isDragging) accent.copy(alpha = 0.18f)
                else accent.copy(alpha = 0.08f),
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
                tint = accent,
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
                    Icon(providerIcon(descriptor.provider), null, modifier = Modifier.size(16.dp), tint = accent)
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

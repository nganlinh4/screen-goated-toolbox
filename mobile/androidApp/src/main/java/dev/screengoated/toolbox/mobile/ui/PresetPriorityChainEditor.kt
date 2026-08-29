@file:OptIn(androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGestures
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
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.preset.AdaptiveManualEdit
import dev.screengoated.toolbox.mobile.preset.ApiKeys
import dev.screengoated.toolbox.mobile.preset.PresetLiveModelOverrides
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelDescriptor
import dev.screengoated.toolbox.mobile.preset.PresetModelFeed
import dev.screengoated.toolbox.mobile.preset.PresetModelType
import dev.screengoated.toolbox.mobile.preset.PresetRetryChainKind
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.preset.adaptiveChain
import dev.screengoated.toolbox.mobile.preset.commitAdaptiveEdits
import dev.screengoated.toolbox.mobile.preset.offeredModels
import dev.screengoated.toolbox.mobile.preset.ui.providerIconRes
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlin.math.roundToInt

@Composable
internal fun PriorityChainEditor(
    title: String,
    authoredChain: List<String>,
    chainKind: PresetRetryChainKind,
    modelType: PresetModelType,
    defaultChain: List<String>,
    adaptiveEnabled: Boolean,
    overrides: PresetLiveModelOverrides,
    settings: PresetRuntimeSettings,
    apiKeys: ApiKeys,
    locale: MobileLocaleText,
    uiLanguage: String,
    accent: Color,
    onStateChanged: (List<String>, Boolean, PresetLiveModelOverrides) -> Unit,
) {
    val feedSnapshot by PresetModelFeed.state.collectAsState()
    val availableModels = PresetModelCatalog.forType(modelType)
    val liveOffers = if (adaptiveEnabled) {
        offeredModels(settings, apiKeys, modelType)
    } else {
        emptyList()
    }
    val liveIds = liveOffers.map { it.first }
    val liveLatencyById = liveOffers
        .filter { (_, latencyMs) -> latencyMs != Int.MAX_VALUE }
        .toMap()
    val visibleChain = if (adaptiveEnabled) {
        chainKind.adaptiveChain(authoredChain, overrides, settings, apiKeys)
    } else {
        authoredChain
    }
    var showAddMenu by remember { mutableStateOf(false) }
    var draggedModelId by remember { mutableStateOf<String?>(null) }
    var dragOffsetY by remember { mutableFloatStateOf(0f) }
    val itemHeightPx = with(LocalDensity.current) { 44.dp.toPx() }
    val latestChain by rememberUpdatedState(visibleChain)
    val latestCommit by rememberUpdatedState(onStateChanged)

    fun commit(chain: List<String>, edit: AdaptiveManualEdit) {
        if (!adaptiveEnabled) {
            latestCommit(chain, false, overrides)
            return
        }
        val result = commitAdaptiveEdits(chain, overrides, liveIds, listOf(edit))
        latestCommit(result.authored, result.remainsEnabled, result.overrides)
    }

    ExpressiveDialogSectionCard(accent = accent, modifier = Modifier.fillMaxWidth()) {
        UtilityHeaderRow(
            icon = if (modelType == PresetModelType.VISION) {
                R.drawable.ms_auto_awesome
            } else {
                R.drawable.ms_translate
            },
            title = title,
            accent = accent,
            trailing = {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    FilterChip(
                        selected = adaptiveEnabled,
                        onClick = {
                            if (adaptiveEnabled) {
                                latestCommit(visibleChain, false, overrides)
                            } else {
                                latestCommit(authoredChain, true, overrides)
                            }
                        },
                        label = { Text(locale.presetRuntimeLive) },
                    )
                    TextButton(
                        onClick = {
                            latestCommit(defaultChain, true, PresetLiveModelOverrides())
                        },
                    ) {
                        Icon(
                            painterResource(R.drawable.ms_settings_backup_restore),
                            null,
                            modifier = Modifier.size(16.dp),
                            tint = accent,
                        )
                        Spacer(Modifier.width(6.dp))
                        Text(locale.presetRuntimeRestoreDefault, color = accent)
                    }
                }
            },
        )

        FixedPriorityEntry(
            number = 0,
            label = locale.presetRuntimeChosenModel,
            hint = locale.presetRuntimeChosenHint,
            accent = accent,
        )

        visibleChain.forEachIndexed { index, modelId ->
            key("$modelId-$index-${feedSnapshot.revision}") {
                DraggablePriorityModel(
                    number = index + 1,
                    modelId = modelId,
                    availableModels = availableModels,
                    liveLatencyById = liveLatencyById,
                    uiLanguage = uiLanguage,
                    accent = accent,
                    isDragging = draggedModelId == modelId,
                    dragOffsetY = dragOffsetY,
                    onModelChanged = { newId ->
                        val list = latestChain.toMutableList()
                        val modelIndex = list.indexOf(modelId)
                        if (modelIndex >= 0) {
                            list[modelIndex] = newId
                            commit(list, AdaptiveManualEdit.Replace(modelId, newId))
                        }
                    },
                    onDragStart = {
                        draggedModelId = modelId
                        dragOffsetY = 0f
                    },
                    onDragDelta = { deltaY ->
                        val activeId = draggedModelId ?: return@DraggablePriorityModel
                        val currentIndex = latestChain.indexOf(activeId)
                        if (currentIndex < 0) {
                            draggedModelId = null
                            dragOffsetY = 0f
                        } else {
                            dragOffsetY += deltaY
                            val steps = (dragOffsetY / itemHeightPx).roundToInt()
                            if (steps != 0) {
                                val target = (currentIndex + steps).coerceIn(0, latestChain.lastIndex)
                                if (target != currentIndex) {
                                    val list = latestChain.toMutableList()
                                    list.add(target, list.removeAt(currentIndex))
                                    dragOffsetY -= (target - currentIndex) * itemHeightPx
                                    commit(list, AdaptiveManualEdit.Move(activeId))
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
                        if (list.remove(modelId)) {
                            if (draggedModelId == modelId) draggedModelId = null
                            dragOffsetY = 0f
                            commit(list, AdaptiveManualEdit.Remove(modelId))
                        }
                    },
                )
            }
        }

        FixedPriorityEntry(
            number = visibleChain.size + 1,
            label = locale.presetRuntimeAuto,
            hint = locale.presetRuntimeAutoHint,
            accent = accent,
        )

        Box {
            TextButton(onClick = { showAddMenu = true }) {
                Text(locale.presetRuntimeAddModel, color = accent)
            }
            DropdownMenu(
                expanded = showAddMenu,
                onDismissRequest = { showAddMenu = false },
                modifier = Modifier.heightIn(max = 420.dp),
            ) {
                availableModels.filter { it.id !in visibleChain }.forEach { model ->
                    PriorityModelDropdownItem(model, uiLanguage, liveLatencyById[model.id]) {
                        commit(visibleChain + model.id, AdaptiveManualEdit.Add(model.id))
                        showAddMenu = false
                    }
                }
            }
        }
    }
}

@Composable
private fun DraggablePriorityModel(
    number: Int,
    modelId: String,
    availableModels: List<PresetModelDescriptor>,
    liveLatencyById: Map<String, Int>,
    uiLanguage: String,
    accent: Color,
    isDragging: Boolean,
    dragOffsetY: Float,
    onDragStart: () -> Unit,
    onDragDelta: (Float) -> Unit,
    onDragEnd: () -> Unit,
    onModelChanged: (String) -> Unit,
    onRemove: () -> Unit,
) {
    val currentStart by rememberUpdatedState(onDragStart)
    val currentDelta by rememberUpdatedState(onDragDelta)
    val currentEnd by rememberUpdatedState(onDragEnd)
    val dragModifier = Modifier
        .width(44.dp)
        .heightIn(min = 32.dp)
        .pointerInput(modelId) {
            detectDragGestures(
                onDragStart = { currentStart() },
                onDrag = { change, offset ->
                    change.consume()
                    currentDelta(offset.y)
                },
                onDragEnd = currentEnd,
                onDragCancel = currentEnd,
            )
        }
    PriorityModelRow(
        number = number,
        modelId = modelId,
        availableModels = availableModels,
        liveLatencyById = liveLatencyById,
        uiLanguage = uiLanguage,
        accent = accent,
        isDragging = isDragging,
        modifier = if (isDragging) {
            Modifier.offset { IntOffset(0, dragOffsetY.roundToInt()) }.zIndex(10f)
        } else {
            Modifier
        },
        dragHandleModifier = dragModifier,
        onModelChanged = onModelChanged,
        onRemove = onRemove,
    )
}

@Composable
private fun FixedPriorityEntry(number: Int, label: String, hint: String, accent: Color) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(MaterialTheme.shapes.small)
            .background(accent.copy(alpha = 0.08f))
            .padding(vertical = 6.dp, horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("$number.", style = MaterialTheme.typography.labelMedium)
        Spacer(Modifier.width(6.dp))
        Text(label, style = MaterialTheme.typography.bodyMedium)
        Spacer(Modifier.width(6.dp))
        Text("→", color = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.width(6.dp))
        Text(hint, style = MaterialTheme.typography.bodySmall)
    }
}

@Composable
private fun PriorityModelRow(
    number: Int,
    modelId: String,
    availableModels: List<PresetModelDescriptor>,
    liveLatencyById: Map<String, Int>,
    uiLanguage: String,
    accent: Color,
    isDragging: Boolean,
    modifier: Modifier,
    dragHandleModifier: Modifier,
    onModelChanged: (String) -> Unit,
    onRemove: () -> Unit,
) {
    val descriptor = PresetModelCatalog.getById(modelId)
    var showDropdown by remember { mutableStateOf(false) }
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(MaterialTheme.shapes.small)
            .background(accent.copy(alpha = if (isDragging) 0.18f else 0.08f))
            .padding(start = 12.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(dragHandleModifier, contentAlignment = Alignment.Center) {
            Icon(
                painterResource(R.drawable.ms_drag_indicator),
                contentDescription = null,
                modifier = Modifier.size(20.dp),
                tint = accent,
            )
        }
        Spacer(Modifier.width(4.dp))
        Text("$number.", style = MaterialTheme.typography.labelMedium)
        Spacer(Modifier.width(6.dp))
        Box(Modifier.weight(1f)) {
            TextButton(onClick = { showDropdown = true }, modifier = Modifier.fillMaxWidth()) {
                ModelPerformancePrefix(descriptor, latencyOverrideMs = liveLatencyById[modelId])
                Spacer(Modifier.width(4.dp))
                descriptor?.let {
                    Icon(painterResource(providerIconRes(it.provider)), null, Modifier.size(16.dp))
                    Spacer(Modifier.width(4.dp))
                }
                Text(
                    descriptor?.localizedName(uiLanguage) ?: modelId,
                    maxLines = 1,
                    modifier = Modifier.weight(1f),
                )
            }
            DropdownMenu(
                expanded = showDropdown,
                onDismissRequest = { showDropdown = false },
                modifier = Modifier.heightIn(max = 420.dp),
            ) {
                availableModels.forEach { model ->
                    PriorityModelDropdownItem(model, uiLanguage, liveLatencyById[model.id]) {
                        onModelChanged(model.id)
                        showDropdown = false
                    }
                }
            }
        }
        IconButton(onClick = onRemove, modifier = Modifier.size(28.dp)) {
            Icon(painterResource(R.drawable.ms_close), null, Modifier.size(14.dp))
        }
    }
}

@Composable
private fun PriorityModelDropdownItem(
    model: PresetModelDescriptor,
    uiLanguage: String,
    latencyOverrideMs: Int? = null,
    onClick: () -> Unit,
) {
    DropdownMenuItem(
        leadingIcon = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                ModelPerformancePrefix(model, latencyOverrideMs = latencyOverrideMs)
                Spacer(Modifier.width(6.dp))
                Icon(painterResource(providerIconRes(model.provider)), null, Modifier.size(18.dp))
            }
        },
        text = {
            Column {
                Text(model.localizedName(uiLanguage))
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

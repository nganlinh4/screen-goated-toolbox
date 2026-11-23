@file:OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ElevatedAssistChip
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.model.CustomModelStatus
import dev.screengoated.toolbox.mobile.model.CustomModelEditorSnapshot
import dev.screengoated.toolbox.mobile.model.CustomModelSubmission
import dev.screengoated.toolbox.mobile.model.CustomModelRow
import dev.screengoated.toolbox.mobile.model.CustomModelRowIdentity
import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelDescriptor
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.PresetModelSource
import dev.screengoated.toolbox.mobile.preset.PresetModelType
import dev.screengoated.toolbox.mobile.preset.ui.providerIconRes
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

private val CustomModelDialogMorphPair = ExpressiveMorphPair(
    from = MaterialShapes.Pill,
    to = MaterialShapes.Cookie6Sided,
)

@Composable
internal fun CustomModelsDialog(
    snapshot: CustomModelEditorSnapshot,
    openRouterApiKey: String,
    ollamaBaseUrl: String,
    locale: MobileLocaleText,
    uiLanguage: String,
    onDismiss: () -> Unit,
    onSave: (List<CustomPresetModelDefinition>, List<CustomPresetModelDefinition>) -> CustomModelSubmission,
    onEditRow: (CustomModelRowIdentity, (CustomPresetModelDefinition) -> CustomPresetModelDefinition) -> CustomModelSubmission,
    onDeleteRow: (CustomModelRowIdentity) -> CustomModelSubmission,
    onRetry: () -> Boolean,
) {
    val models = snapshot.models
    val status = snapshot.status
    val dialogLifetime = remember { CustomModelActionLifetime() }
    DisposableEffect(dialogLifetime) { onDispose { dialogLifetime.retire() } }
    fun dismiss() { dialogLifetime.retire(); onDismiss() }
    if (status == CustomModelStatus.DATA_UNREADABLE || status == CustomModelStatus.WRITE_UNCERTAIN) {
        ExpressiveDialogSurface(
            title = locale.customModelsTitle,
            closeLabel = locale.closeLabel,
            supporting = locale.customModelsDescription,
            icon = R.drawable.ms_tune,
            accent = MaterialTheme.colorScheme.primary,
            morphPair = CustomModelDialogMorphPair,
            onDismiss = ::dismiss,
            widthFraction = 0.96f,
            maxWidth = 720.dp,
            heightFraction = 0.84f,
            maxHeight = 760.dp,
        ) {
            val message = if (status == CustomModelStatus.WRITE_UNCERTAIN) locale.customModelsSaveUncertain
                else locale.customModelsUnavailable
            Text(message, color = MaterialTheme.colorScheme.error)
            if (status == CustomModelStatus.WRITE_UNCERTAIN) {
                Button(onClick = { dialogLifetime.run(onRetry) }) { Text(locale.customModelsRetry) }
            }
        }
        return
    }
    val draft = remember { mutableStateListOf<CustomPresetModelDefinition>().apply { addAll(models) } }
    val editorRows = remember { mutableStateListOf<CustomModelRow>().apply { addAll(snapshot.rows) } }
    val revision = remember { CustomModelEditorRevision(snapshot.revision) }
    val editorLifetime = remember { CustomModelActionLifetime() }
    DisposableEffect(editorLifetime) { onDispose { editorLifetime.retire() } }
    val scope = rememberCoroutineScope()
    var importModels by remember { mutableStateOf<List<ImportableOpenRouterModel>>(emptyList()) }
    var statusText by remember { mutableStateOf<String?>(null) }
    var importing by remember { mutableStateOf(false) }
    val accent = MaterialTheme.colorScheme.primary

    LaunchedEffect(snapshot) {
        if (revision.admit(snapshot.revision)) {
            draft.clear()
            draft.addAll(models)
            editorRows.clear()
            editorRows.addAll(snapshot.rows)
        }
    }

    fun accept(result: CustomModelSubmission): Boolean {
        val accepted = when (result) {
            is CustomModelSubmission.Accepted -> result.snapshot
            is CustomModelSubmission.Conflict -> {
                if (revision.admit(result.snapshot.revision)) {
                    draft.clear(); draft.addAll(result.snapshot.models)
                    editorRows.clear(); editorRows.addAll(result.snapshot.rows)
                }
                statusText = locale.customModelsChangedElsewhere
                return false
            }
            CustomModelSubmission.Unavailable -> { statusText = locale.customModelsSaveUncertain; return false }
        }
        if (revision.admit(accepted.revision)) {
            draft.clear()
            draft.addAll(accepted.models)
            editorRows.clear()
            editorRows.addAll(accepted.rows)
        }
        statusText = null
        return true
    }

    fun perform(action: () -> CustomModelSubmission): Boolean =
        dialogLifetime.run { editorLifetime.run { accept(action()) } }

    fun commit(next: List<CustomPresetModelDefinition>): Boolean = perform { onSave(draft.toList(), next) }

    ExpressiveDialogSurface(
        title = locale.customModelsTitle,
        closeLabel = locale.closeLabel,
        supporting = locale.customModelsDescription,
        icon = R.drawable.ms_tune,
        accent = accent,
        morphPair = CustomModelDialogMorphPair,
        onDismiss = ::dismiss,
        widthFraction = 0.96f,
        maxWidth = 720.dp,
        heightFraction = 0.84f,
        maxHeight = 760.dp,
    ) {
        if (status == CustomModelStatus.SAVING) Text(locale.customModelsSaving, style = MaterialTheme.typography.bodySmall)
        statusText?.let {
            Text(
                text = it,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
            )
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            CUSTOM_MODELS_EDITABLE_PROVIDERS.forEach { provider ->
                ProviderModelSection(
                    provider = provider,
                    draft = draft,
                    rows = editorRows,
                    importModels = if (provider == PresetModelProvider.OPENROUTER) {
                        importModels
                    } else {
                        emptyList()
                    },
                    importing = importing,
                    locale = locale,
                    uiLanguage = uiLanguage,
                    onAddModel = { model ->
                        commit(draft.toList() + model)
                    },
                    onChangeModel = { identity, transform ->
                        perform { onEditRow(identity, transform) }
                    },
                    onDeleteModel = { identity ->
                        perform { onDeleteRow(identity) }
                    },
                    onImportOpenRouter = {
                        importing = true
                        statusText = null
                        scope.launch {
                            try {
                                importModels = fetchOpenRouterModels(openRouterApiKey)
                            } catch (cancelled: CancellationException) {
                                throw cancelled
                            } catch (error: Exception) {
                                statusText = error.message
                            } finally { importing = false }
                        }
                    },
                    onScanOllama = {
                        importing = true
                        statusText = null
                        scope.launch {
                            try {
                                val scanned = fetchOllamaModels(ollamaBaseUrl)
                                val additions = planOllamaScan(scanned, draft)
                                if (commit(draft.toList() + additions)) {
                                    statusText = ollamaStatusText(scanned.size, additions.size)
                                }
                            } catch (cancelled: CancellationException) {
                                throw cancelled
                            } catch (error: Exception) {
                                statusText = error.message
                            } finally { importing = false }
                        }
                    },
                )
            }
        }
    }
}

@Composable
private fun ImportResultCard(
    models: List<ImportableOpenRouterModel>,
    draft: MutableList<CustomPresetModelDefinition>,
    locale: MobileLocaleText,
    onAddModel: (CustomPresetModelDefinition) -> Unit,
) {
    val accent = providerColor(PresetModelProvider.OPENROUTER)
    var query by remember { mutableStateOf("") }
    val visible = remember(models, query) { filterOpenRouterImports(models, query) }
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(
            containerColor = lerp(MaterialTheme.colorScheme.surfaceContainerLow, accent, 0.10f),
        ),
        border = BorderStroke(1.dp, accent.copy(alpha = 0.22f)),
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                placeholder = { Text(locale.customModelsImportSearch) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            visible.forEach { model ->
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    ElevatedAssistChip(
                        onClick = {
                            if (draft.none {
                                    it.provider == PresetModelProvider.OPENROUTER &&
                                        it.fullName == model.id
                                }
                            ) {
                                onAddModel(importedOpenRouterModel(model, draft))
                            }
                        },
                        label = { Text("+") },
                    )
                    Column(modifier = Modifier.weight(1f)) {
                        Text(model.name, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(
                            model.id,
                            fontFamily = FontFamily.Monospace,
                            style = MaterialTheme.typography.bodySmall,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    TypeBadge(modelTypeLabel(model.type, locale))
                }
            }
        }
    }
}

@Composable
private fun ProviderModelSection(
    provider: PresetModelProvider,
    draft: MutableList<CustomPresetModelDefinition>,
    rows: List<CustomModelRow>,
    importModels: List<ImportableOpenRouterModel>,
    importing: Boolean,
    locale: MobileLocaleText,
    uiLanguage: String,
    onAddModel: (CustomPresetModelDefinition) -> Unit,
    onChangeModel: (CustomModelRowIdentity, (CustomPresetModelDefinition) -> CustomPresetModelDefinition) -> Unit,
    onDeleteModel: (CustomModelRowIdentity) -> Unit,
    onImportOpenRouter: () -> Unit,
    onScanOllama: () -> Unit,
) {
    val allModels = PresetModelCatalog.dialogModels().filter { it.provider == provider }
    val builtIns = allModels.filter { it.source == PresetModelSource.BUILT_IN }
    val discovered = allModels.filter { it.source == PresetModelSource.DISCOVERED }
    val userModels = rows.filter { it.model.provider == provider }
    val accent = providerColor(provider)

    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(
            containerColor = lerp(MaterialTheme.colorScheme.surfaceContainerLow, accent, 0.045f),
        ),
        border = BorderStroke(1.dp, accent.copy(alpha = 0.18f)),
    ) {
        Column(
            modifier = Modifier.padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                ProviderBadge(provider, accent)
                Text(
                    text = providerLabel(provider),
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.weight(1f),
                )
            }
            ProviderSectionActions(
                provider = provider,
                draft = draft,
                importing = importing,
                locale = locale,
                onAddModel = onAddModel,
                onImportOpenRouter = onImportOpenRouter,
                onScanOllama = onScanOllama,
            )

            if (provider == PresetModelProvider.OPENROUTER && importModels.isNotEmpty()) {
                ImportResultCard(
                    models = importModels,
                    draft = draft,
                    locale = locale,
                    onAddModel = onAddModel,
                )
            }

            if (builtIns.isNotEmpty()) {
                SectionCaption(locale.customModelsBuiltinLocked)
                builtIns.forEach { LockedModelRow(it, locale, uiLanguage) }
            }

            if (userModels.isNotEmpty()) {
                SectionCaption(locale.customModelsUserModels)
                userModels.forEach { row ->
                    key(row.identity) {
                        CustomModelEditor(
                            model = row.model,
                            locale = locale,
                            accent = accent,
                            onChange = { onChangeModel(row.identity, it) },
                            onDelete = { onDeleteModel(row.identity) },
                        )
                    }
                }
            }

            if (discovered.isNotEmpty()) {
                SectionCaption(locale.customModelsDiscoveredModels)
                discovered.forEach { LockedModelRow(it, locale, uiLanguage) }
            } else if (builtIns.isEmpty() && userModels.isEmpty()) {
                Text(
                    text = locale.customModelsNoModels,
                    modifier = Modifier.fillMaxWidth(),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodyMedium,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

@Composable
private fun ProviderSectionActions(
    provider: PresetModelProvider,
    draft: List<CustomPresetModelDefinition>,
    importing: Boolean,
    locale: MobileLocaleText,
    onAddModel: (CustomPresetModelDefinition) -> Unit,
    onImportOpenRouter: () -> Unit,
    onScanOllama: () -> Unit,
) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (provider == PresetModelProvider.OPENROUTER) {
            AssistChip(
                onClick = { onAddModel(newCustomModel(provider, draft)) },
                label = { Text(addLabel(provider, locale), maxLines = 1) },
            )
            AssistChip(
                enabled = !importing,
                onClick = onImportOpenRouter,
                label = { Text(locale.customModelsScan, maxLines = 1) },
            )
        } else if (provider == PresetModelProvider.OLLAMA) {
            AssistChip(
                enabled = !importing,
                onClick = onScanOllama,
                label = { Text(locale.customModelsScan, maxLines = 1) },
            )
        } else {
            AssistChip(
                onClick = { onAddModel(newCustomModel(provider, draft)) },
                label = { Text(addLabel(provider, locale), maxLines = 1) },
            )
        }
    }
}

@Composable
private fun ProviderBadge(provider: PresetModelProvider, accent: Color) {
    androidx.compose.foundation.layout.Box(
        modifier = Modifier
            .size(36.dp)
            .background(accent.copy(alpha = 0.18f), CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painter = painterResource(providerIconRes(provider)),
            contentDescription = null,
            tint = accent,
            modifier = Modifier.size(19.dp),
        )
    }
}

@Composable
private fun SectionCaption(label: String) {
    Text(
        text = label,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun LockedModelRow(
    model: PresetModelDescriptor,
    locale: MobileLocaleText,
    uiLanguage: String,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(
                MaterialTheme.colorScheme.surfaceContainerHighest.copy(alpha = 0.45f),
                MaterialTheme.shapes.medium,
            )
            .padding(horizontal = 10.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        ModelPerformancePrefix(model)
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = lockedModelDisplayName(model, uiLanguage),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = model.fullName,
                fontFamily = FontFamily.Monospace,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        TypeBadge(modelTypeLabel(model.modelType, locale))
    }
}

internal fun lockedModelDisplayName(model: PresetModelDescriptor, uiLanguage: String): String =
    model.localizedName(uiLanguage)

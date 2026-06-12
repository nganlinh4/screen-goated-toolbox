package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.PresetModelSource
import dev.screengoated.toolbox.mobile.preset.PresetModelType
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject

private data class ImportableOpenRouterModel(
    val id: String,
    val name: String,
    val type: PresetModelType,
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun CustomModelsDialog(
    models: List<CustomPresetModelDefinition>,
    openRouterApiKey: String,
    locale: MobileLocaleText,
    onDismiss: () -> Unit,
    onSave: (List<CustomPresetModelDefinition>) -> Unit,
) {
    val draft = remember { mutableStateListOf<CustomPresetModelDefinition>() }
    val scope = rememberCoroutineScope()
    var importModels by remember { mutableStateOf<List<ImportableOpenRouterModel>>(emptyList()) }
    var importError by remember { mutableStateOf<String?>(null) }
    var importing by remember { mutableStateOf(false) }

    LaunchedEffect(models) {
        draft.clear()
        draft.addAll(models)
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(locale.customModelsTitle) },
        text = {
            Column(
                modifier = Modifier
                    .heightIn(max = 560.dp)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Text(locale.customModelsDescription)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(
                        onClick = {
                            draft.add(newOpenRouterModel(draft))
                        },
                    ) {
                        Text(locale.customModelsAddOpenRouter)
                    }
                    Button(
                        enabled = !importing,
                        onClick = {
                            importing = true
                            importError = null
                            scope.launch {
                                runCatching {
                                    fetchOpenRouterModels(openRouterApiKey)
                                }.onSuccess {
                                    importModels = it
                                }.onFailure {
                                    importError = it.message
                                }
                                importing = false
                            }
                        },
                    ) {
                        Text(if (importing) "..." else locale.customModelsImportOpenRouter)
                    }
                }
                importError?.let { Text(it) }
                importModels.take(30).forEach { model ->
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        TextButton(
                            onClick = {
                                if (draft.none { it.provider == PresetModelProvider.OPENROUTER && it.fullName == model.id }) {
                                    draft.add(importedOpenRouterModel(model, draft))
                                }
                            },
                        ) {
                            Text("+")
                        }
                        Column(modifier = Modifier.weight(1f)) {
                            Text(model.name, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(model.id, fontFamily = FontFamily.Monospace, maxLines = 1)
                        }
                    }
                }

                lockedProviderSummary()

                draft.forEachIndexed { index, model ->
                    CustomModelEditor(
                        model = model,
                        locale = locale,
                        onChange = { draft[index] = it },
                        onDelete = { draft.removeAt(index) },
                    )
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    onSave(draft.toList())
                    onDismiss()
                },
            ) {
                Text(locale.customModelsSave)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(locale.presetRuntimeCancel)
            }
        },
    )
}

@Composable
private fun lockedProviderSummary() {
    val builtIns = PresetModelCatalog.dialogModels()
        .filter { it.source == PresetModelSource.BUILT_IN }
        .groupBy { it.provider }
    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
        builtIns.forEach { (provider, models) ->
            Text("${provider.name}: ${models.size} built-in")
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CustomModelEditor(
    model: CustomPresetModelDefinition,
    locale: MobileLocaleText,
    onChange: (CustomPresetModelDefinition) -> Unit,
    onDelete: () -> Unit,
) {
    var typeExpanded by remember { mutableStateOf(false) }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Checkbox(
                checked = model.enabled,
                onCheckedChange = { onChange(model.copy(enabled = it)) },
            )
            Text(locale.customModelsEnabled)
            Checkbox(
                checked = model.supportsSearch == true,
                onCheckedChange = { onChange(model.copy(supportsSearch = it)) },
            )
            Text(locale.customModelsSearch)
            TextButton(onClick = onDelete) {
                Text(locale.customModelsDelete)
            }
        }
        OutlinedTextField(
            value = model.displayName,
            onValueChange = { onChange(model.copy(displayName = it)) },
            label = { Text(locale.customModelsDisplayName) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = model.fullName,
            onValueChange = { onChange(model.copy(fullName = it)) },
            label = { Text(locale.customModelsApiModel) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        ExposedDropdownMenuBox(
            expanded = typeExpanded,
            onExpandedChange = { typeExpanded = it },
        ) {
            OutlinedTextField(
                value = modelTypeLabel(model.modelType, locale),
                onValueChange = {},
                readOnly = true,
                label = { Text(locale.customModelsType) },
                trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = typeExpanded) },
                modifier = Modifier
                    .menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable, enabled = true)
                    .fillMaxWidth(),
            )
            ExposedDropdownMenu(
                expanded = typeExpanded,
                onDismissRequest = { typeExpanded = false },
            ) {
                DropdownMenuItem(
                    text = { Text(locale.customModelsTextType) },
                    onClick = {
                        onChange(model.copy(modelType = PresetModelType.TEXT))
                        typeExpanded = false
                    },
                )
                DropdownMenuItem(
                    text = { Text(locale.customModelsVisionType) },
                    onClick = {
                        onChange(model.copy(modelType = PresetModelType.VISION))
                        typeExpanded = false
                    },
                )
            }
        }
        Text(model.id, fontFamily = FontFamily.Monospace)
    }
}

private suspend fun fetchOpenRouterModels(apiKey: String): List<ImportableOpenRouterModel> =
    withContext(Dispatchers.IO) {
        val builder = Request.Builder()
            .url("https://openrouter.ai/api/v1/models")
        if (apiKey.isNotBlank()) {
            builder.header("Authorization", "Bearer ${apiKey.trim()}")
        }
        OkHttpClient().newCall(builder.build()).execute().use { response ->
            if (!response.isSuccessful) {
                error("OpenRouter scan failed: HTTP ${response.code}")
            }
            val body = response.body?.string().orEmpty()
            val json = JSONObject(body)
            val data = json.optJSONArray("data") ?: return@withContext emptyList()
            List(data.length()) { index ->
                val item = data.getJSONObject(index)
                val id = item.getString("id")
                ImportableOpenRouterModel(
                    id = id,
                    name = item.optString("name", id),
                    type = inferModelType(item, id),
                )
            }
        }
    }

private fun inferModelType(item: JSONObject, id: String): PresetModelType {
    val text = "${item} $id".lowercase()
    return if (
        text.contains("\"image\"") ||
        text.contains("vision") ||
        text.contains("-vl") ||
        text.contains("/vl") ||
        text.contains("llava")
    ) {
        PresetModelType.VISION
    } else {
        PresetModelType.TEXT
    }
}

private fun importedOpenRouterModel(
    model: ImportableOpenRouterModel,
    existing: List<CustomPresetModelDefinition>,
): CustomPresetModelDefinition = CustomPresetModelDefinition(
    id = uniqueCustomId("openrouter", model.id, existing),
    provider = PresetModelProvider.OPENROUTER,
    displayName = model.name,
    fullName = model.id,
    modelType = model.type,
    quotaEn = "OpenRouter quota",
    quotaVi = "Theo OpenRouter",
    quotaKo = "OpenRouter 기준",
)

private fun newOpenRouterModel(existing: List<CustomPresetModelDefinition>): CustomPresetModelDefinition {
    val fullName = "provider/model"
    return CustomPresetModelDefinition(
        id = uniqueCustomId("openrouter", fullName, existing),
        provider = PresetModelProvider.OPENROUTER,
        displayName = "OpenRouter",
        fullName = fullName,
        quotaEn = "OpenRouter quota",
        quotaVi = "Theo OpenRouter",
        quotaKo = "OpenRouter 기준",
    )
}

private fun uniqueCustomId(
    provider: String,
    fullName: String,
    existing: List<CustomPresetModelDefinition>,
): String {
    val base = "custom-$provider-${slugify(fullName)}"
    var candidate = base
    var suffix = 2
    while (existing.any { it.id == candidate } || PresetModelCatalog.getById(candidate) != null) {
        candidate = "$base-$suffix"
        suffix += 1
    }
    return candidate
}

private fun slugify(value: String): String =
    value.lowercase()
        .replace(Regex("[^a-z0-9]+"), "-")
        .trim('-')

private fun modelTypeLabel(type: PresetModelType, locale: MobileLocaleText): String = when (type) {
    PresetModelType.VISION -> locale.customModelsVisionType
    PresetModelType.TEXT -> locale.customModelsTextType
    PresetModelType.AUDIO -> locale.dlAudioLabel
}

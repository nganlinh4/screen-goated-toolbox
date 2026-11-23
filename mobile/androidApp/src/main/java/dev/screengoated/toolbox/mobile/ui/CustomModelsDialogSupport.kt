package dev.screengoated.toolbox.mobile.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.PresetModelType
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject

internal data class ImportableOpenRouterModel(
    val id: String,
    val name: String,
    val type: PresetModelType,
)

/** Rows rendered at once; the search always runs over the whole scanned list. */
internal const val OPENROUTER_IMPORT_RENDER_LIMIT = 30

/** Matches Windows openrouter_import.rs: case-insensitive substring of name or id. */
internal fun filterOpenRouterImports(
    models: List<ImportableOpenRouterModel>,
    query: String,
    limit: Int = OPENROUTER_IMPORT_RENDER_LIMIT,
): List<ImportableOpenRouterModel> {
    val needle = query.lowercase()
    return models.asSequence()
        .filter { needle.isEmpty() || it.name.lowercase().contains(needle) || it.id.lowercase().contains(needle) }
        .take(limit)
        .toList()
}

internal val CUSTOM_MODELS_EDITABLE_PROVIDERS = listOf(
    PresetModelProvider.GOOGLE,
    PresetModelProvider.GROQ,
    PresetModelProvider.OPENROUTER,
    PresetModelProvider.OLLAMA,
)

internal suspend fun fetchOpenRouterModels(apiKey: String): List<ImportableOpenRouterModel> =
    withContext(Dispatchers.IO) {
        val builder = Request.Builder().url("https://openrouter.ai/api/v1/models")
        if (apiKey.isNotBlank()) {
            builder.header("Authorization", "Bearer ${apiKey.trim()}")
        }
        readScanResponse(OkHttpClient().newCall(builder.build())) { response ->
            if (!response.isSuccessful) {
                error("OpenRouter scan failed: HTTP ${response.code}")
            }
            val data = JSONObject(readBoundedScanText(response.body)).optJSONArray("data")
                ?: return@readScanResponse emptyList()
            List(data.length()) { index ->
                val item = data.getJSONObject(index)
                val id = item.getString("id")
                ImportableOpenRouterModel(
                    id = id,
                    name = item.optString("name", id),
                    type = inferOpenRouterModelType(item),
                )
            }
        }
    }

internal suspend fun fetchOllamaModels(baseUrl: String): List<CustomPresetModelDefinition> =
    withContext(Dispatchers.IO) {
        val normalized = baseUrl.trim().trimEnd('/')
        require(normalized.isNotBlank()) { "Set the Ollama URL in API Keys first." }
        val request = Request.Builder().url("$normalized/api/tags").build()
        readScanResponse(OkHttpClient().newCall(request)) { response ->
            if (!response.isSuccessful) {
                error("Ollama scan failed: HTTP ${response.code}")
            }
            val models = JSONObject(readBoundedScanText(response.body)).optJSONArray("models")
                ?: return@readScanResponse emptyList()
            List(models.length()) { index ->
                val name = models.getJSONObject(index).getString("name")
                newCustomModel(PresetModelProvider.OLLAMA, emptyList(), name)
            }
        }
    }

internal fun inferOpenRouterModelType(item: JSONObject): PresetModelType {
    val input = item.optJSONObject("architecture")?.optJSONArray("input_modalities")
    return if (input != null && (0 until input.length()).any { input.opt(it) == "image" }) {
        PresetModelType.VISION
    } else {
        PresetModelType.TEXT
    }
}

internal fun importedOpenRouterModel(
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

internal fun planOllamaScan(
    scanned: List<CustomPresetModelDefinition>,
    existing: List<CustomPresetModelDefinition>,
): List<CustomPresetModelDefinition> {
    val combined = existing.toMutableList()
    val additions = mutableListOf<CustomPresetModelDefinition>()
    for (candidate in scanned) {
        if (combined.any { it.provider == PresetModelProvider.OLLAMA && it.fullName == candidate.fullName }) continue
        val added = candidate.copy(id = uniqueCustomId("ollama", candidate.fullName, combined))
        combined.add(added)
        additions.add(added)
    }
    return additions
}

internal fun newCustomModel(
    provider: PresetModelProvider,
    existing: List<CustomPresetModelDefinition>,
    fullNameOverride: String? = null,
): CustomPresetModelDefinition {
    val providerId = provider.name.lowercase()
    val display = providerLabel(provider)
    val fullName = fullNameOverride ?: "$providerId/model"
    return CustomPresetModelDefinition(
        id = uniqueCustomId(providerId, fullName, existing),
        provider = provider,
        displayName = fullNameOverride ?: display,
        fullName = fullName,
        quotaEn = "$display quota",
        quotaVi = "Theo $display",
        quotaKo = "$display 기준",
    )
}

internal fun uniqueCustomId(
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

internal fun modelTypeLabel(type: PresetModelType, locale: MobileLocaleText): String = when (type) {
    PresetModelType.VISION -> locale.customModelsVisionType
    PresetModelType.TEXT -> locale.customModelsTextType
    PresetModelType.AUDIO -> locale.dlAudioLabel
}

internal fun addLabel(provider: PresetModelProvider, locale: MobileLocaleText): String =
    if (provider == PresetModelProvider.OPENROUTER) {
        locale.customModelsAdd
    } else {
        locale.customModelsAdd
    }

internal fun providerLabel(provider: PresetModelProvider): String = when (provider) {
    PresetModelProvider.GOOGLE -> "Gemini"
    PresetModelProvider.GROQ -> "Groq"
    PresetModelProvider.OPENROUTER -> "OpenRouter"
    PresetModelProvider.OLLAMA -> "Ollama"
    else -> provider.name
}

@Composable
internal fun providerColor(provider: PresetModelProvider): Color =
    providerAccent(providerLabel(provider), MaterialTheme.colorScheme)

internal fun ollamaStatusText(scanned: Int, added: Int): String =
    "Ollama scan finished: $scanned found, $added added."

package dev.screengoated.toolbox.mobile.phonecontrol.authorization

import android.content.Context
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelType
import dev.screengoated.toolbox.mobile.preset.PresetRetryChainKind
import dev.screengoated.toolbox.mobile.preset.TextApiClient
import dev.screengoated.toolbox.mobile.preset.effectiveChain
import dev.screengoated.toolbox.mobile.preset.preflightSkipReason
import java.util.concurrent.TimeUnit
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

internal fun interface RequestContractCandidateClient {
    suspend fun request(
        modelId: String,
        instruction: String,
        context: String,
    ): Result<String>
}

internal class AndroidRequestContractCandidates(context: Context) :
    RequestContractCandidateClient {
    private val container =
        (context.applicationContext as SgtMobileApplication).appContainer
    private val client = TextApiClient(
        container.httpClient.newBuilder()
            .connectTimeout(PROVIDER_TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .readTimeout(PROVIDER_TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .writeTimeout(PROVIDER_TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .callTimeout(PROVIDER_TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .build(),
    )

    fun modelIds(): List<String> {
        val settings = container.currentPresetRuntimeSettings()
        val apiKeys = container.currentPresetApiKeys()
        return PresetRetryChainKind.TEXT_TO_TEXT.effectiveChain(settings, apiKeys)
            .distinct()
            .filter { id ->
                val model = PresetModelCatalog.getById(id)
                model?.modelType == PresetModelType.TEXT &&
                    preflightSkipReason(
                        modelId = id,
                        provider = model.provider,
                        apiKeys = apiKeys,
                        blockedProviders = emptySet(),
                        settings = settings,
                    ) == null
            }
    }

    override suspend fun request(
        modelId: String,
        instruction: String,
        context: String,
    ): Result<String> = client.executeStreaming(
        modelId = modelId,
        prompt = instruction,
        inputText = context,
        apiKeys = container.currentPresetApiKeys(),
        uiLanguage = "en",
        searchLabel = null,
        onChunk = {},
        streamingEnabled = false,
    )

    private companion object {
        const val PROVIDER_TIMEOUT_SECONDS = 9L
    }
}

internal data class RequestContractVerdict(
    val authorized: Boolean,
    val reason: String,
)

internal fun parseRequestContractVerdict(answer: String): RequestContractVerdict? =
    balancedJsonObjects(answer).take(MAX_JSON_CANDIDATES).firstNotNullOfOrNull { candidate ->
        val value = runCatching { STRICT_JSON.parseToJsonElement(candidate).jsonObject }.getOrNull()
            ?: return@firstNotNullOfOrNull null
        val authorized = value["authorized"]?.jsonPrimitive?.booleanOrNull
            ?: return@firstNotNullOfOrNull null
        val reason = value["reason"]?.jsonPrimitive?.contentOrNull?.trim().orEmpty()
        reason.takeIf(String::isNotEmpty)?.let {
            RequestContractVerdict(authorized, it.take(MAX_REASON_CHARS))
        }
    }

private fun balancedJsonObjects(text: String): Sequence<String> = sequence {
    text.indices.asSequence().filter { text[it] == '{' }.forEach { start ->
        var depth = 0
        var inString = false
        var escaped = false
        for (index in start until text.length) {
            val character = text[index]
            if (inString) {
                when {
                    escaped -> escaped = false
                    character == '\\' -> escaped = true
                    character == '"' -> inString = false
                }
            } else {
                when (character) {
                    '"' -> inString = true
                    '{' -> depth += 1
                    '}' -> {
                        depth -= 1
                        if (depth == 0) {
                            yield(text.substring(start, index + 1))
                            break
                        }
                    }
                }
            }
        }
    }
}

private val STRICT_JSON = Json { ignoreUnknownKeys = true }
private const val MAX_JSON_CANDIDATES = 64
private const val MAX_REASON_CHARS = 1_024

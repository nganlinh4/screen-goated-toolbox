package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.storage.SecureSettingsStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import okhttp3.Request

class EdgeVoiceCatalogService(
    private val httpClient: OkHttpClient,
    private val settingsStore: SecureSettingsStore,
    private val json: Json,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutableState = MutableStateFlow(
        settingsStore.loadEdgeVoiceCatalog()?.toState()
            ?: EdgeVoiceCatalogState(),
    )

    val state: StateFlow<EdgeVoiceCatalogState> = mutableState.asStateFlow()

    fun ensureLoaded(force: Boolean = false) {
        val current = mutableState.value
        if (!force && (current.loaded || current.loading)) {
            return
        }
        mutableState.value = current.copy(loading = true, errorMessage = null)
        scope.launch {
            runCatching {
                val request = Request.Builder()
                    .url(EDGE_VOICES_URL)
                    .header(
                        "User-Agent",
                        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
                    )
                    .build()

                httpClient.newCall(request).execute().use { response ->
                    check(response.isSuccessful) { "Edge voices HTTP ${response.code}" }
                    val body = response.body?.string().orEmpty()
                    val voices = json.decodeFromString<List<EdgeVoicePayload>>(body).map {
                        EdgeVoice(
                            shortName = it.shortName,
                            gender = it.gender,
                            locale = it.locale,
                            friendlyName = it.friendlyName,
                        )
                    }
                    val cache = CachedEdgeVoiceCatalog(voices)
                    settingsStore.saveEdgeVoiceCatalog(cache)
                    cache.toState()
                }
            }.onSuccess { next ->
                mutableState.value = next
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    loading = false,
                    errorMessage = error.message ?: "Failed to load Edge voices.",
                )
            }
        }
    }

    fun voicesForLanguage(languageCode: String): List<EdgeVoice> {
        return state.value.byLanguage[languageCode.lowercase()] ?: emptyList()
    }

    private fun CachedEdgeVoiceCatalog.toState(): EdgeVoiceCatalogState {
        val byLanguage = voices
            .groupBy { it.locale.substringBefore('-').lowercase() }
            .mapValues { (_, entries) -> entries.sortedBy(EdgeVoice::friendlyName) }
        return EdgeVoiceCatalogState(
            voices = voices.sortedBy(EdgeVoice::friendlyName),
            byLanguage = byLanguage,
            loaded = voices.isNotEmpty(),
            loading = false,
            errorMessage = null,
        )
    }

    @Serializable
    private data class EdgeVoicePayload(
        @SerialName("ShortName")
        val shortName: String,
        @SerialName("Gender")
        val gender: String,
        @SerialName("Locale")
        val locale: String,
        @SerialName("FriendlyName")
        val friendlyName: String,
    )

    private companion object {
        private const val EDGE_VOICES_URL =
            "https://speech.platform.bing.com/consumer/speech/synthesize/readaloud/voices/list?trustedclienttoken=6A5AA1D4EAFF4E9FB37E23D68491D6F4"
    }
}

@file:Suppress("DEPRECATION")

package dev.screengoated.toolbox.mobile.storage

import android.content.Context
import android.content.SharedPreferences
import androidx.core.content.edit
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.translationgummy.TranslationGummyConfig
import dev.screengoated.toolbox.mobile.translationgummy.TranslationGummyTranscriptItem
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.preset.normalizeModelPriorityChains
import dev.screengoated.toolbox.mobile.model.RealtimePaneFontSizes
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.service.tts.CachedEdgeVoiceCatalog
import kotlinx.serialization.json.Json
import java.nio.file.Files
import java.nio.file.LinkOption
import java.security.KeyStore

class SecureSettingsStore internal constructor(
    private val prefs: SharedPreferences,
    private val json: Json,
) {
    constructor(context: Context, json: Json) : this(
        withSecureStoreAccess {
            val sharedPrefs = context.dataDir.toPath().resolve("shared_prefs")
            val storeExists = existingSecureStore { name -> secureStoreFileExists(sharedPrefs.resolve(name)) }
            requireExistingKeyIfStoreExists(storeExists) {
                KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
                    .containsAlias(MasterKey.DEFAULT_MASTER_KEY_ALIAS)
            }
            if (storeExists) {
                Files.newInputStream(sharedPrefs.resolve("$SECURE_SETTINGS_PREFS_NAME.xml"), LinkOption.NOFOLLOW_LINKS)
                    .use(::inspectSecureStoreXml)
            }
            EncryptedSharedPreferences.create(
                context,
                SECURE_SETTINGS_PREFS_NAME,
                MasterKey.Builder(context)
                    .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
                    .build(),
                EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
            )
        },
        json,
    )

    private fun getString(key: String, fallback: String?): String? =
        withSecureStoreAccess { prefs.getString(key, fallback) }

    private fun getInt(key: String, fallback: Int): Int =
        withSecureStoreAccess { prefs.getInt(key, fallback) }

    private fun getBoolean(key: String, fallback: Boolean): Boolean =
        withSecureStoreAccess { prefs.getBoolean(key, fallback) }

    private fun secureEdit(action: android.content.SharedPreferences.Editor.() -> Unit) =
        withSecureStoreAccess { prefs.edit(action = action) }

    fun loadConfig(): LiveSessionConfig {
        val payload = getString(KEY_SESSION_CONFIG, null) ?: return LiveSessionConfig()
        return runCatching {
            json.decodeFromString<LiveSessionConfig>(payload)
        }.getOrDefault(LiveSessionConfig())
    }

    fun saveConfig(config: LiveSessionConfig) {
        secureEdit {
            putString(KEY_SESSION_CONFIG, json.encodeToString(LiveSessionConfig.serializer(), config))
        }
    }

    fun loadTranslationGummyConfig(): TranslationGummyConfig {
        val payload = getString(KEY_TRANSLATION_GUMMY_CONFIG, null)
            ?: getString(LEGACY_KEY_BILINGUAL_RELAY_CONFIG, null)
            ?: return TranslationGummyConfig()
        return runCatching {
            json.decodeFromString<TranslationGummyConfig>(payload)
        }.getOrDefault(TranslationGummyConfig())
    }

    fun saveTranslationGummyConfig(config: TranslationGummyConfig) {
        secureEdit {
            putString(
                KEY_TRANSLATION_GUMMY_CONFIG,
                json.encodeToString(TranslationGummyConfig.serializer(), config),
            )
            remove(LEGACY_KEY_BILINGUAL_RELAY_CONFIG)
        }
    }

    fun loadTranslationGummyTranscripts(): List<TranslationGummyTranscriptItem> {
        val payload = getString(KEY_TRANSLATION_GUMMY_TRANSCRIPTS, null)
            ?: getString(LEGACY_KEY_BILINGUAL_RELAY_TRANSCRIPTS, null)
            ?: return emptyList()
        return runCatching {
            json.decodeFromString<List<TranslationGummyTranscriptItem>>(payload)
        }.getOrDefault(emptyList())
    }

    fun saveTranslationGummyTranscripts(transcripts: List<TranslationGummyTranscriptItem>) {
        secureEdit {
            putString(
                KEY_TRANSLATION_GUMMY_TRANSCRIPTS,
                json.encodeToString(transcripts),
            )
            remove(LEGACY_KEY_BILINGUAL_RELAY_TRANSCRIPTS)
        }
    }

    fun loadApiKey(): String {
        return getString(KEY_GEMINI_API_KEY, "") ?: ""
    }

    fun saveApiKey(apiKey: String) {
        secureEdit {
            putString(KEY_GEMINI_API_KEY, apiKey)
        }
    }

    fun loadGroqApiKey(): String {
        return getString(KEY_GROQ_API_KEY, "") ?: ""
    }

    fun saveGroqApiKey(apiKey: String) {
        secureEdit {
            putString(KEY_GROQ_API_KEY, apiKey)
        }
    }

    fun loadOpenRouterApiKey(): String {
        return getString(KEY_OPENROUTER_API_KEY, "") ?: ""
    }

    fun saveOpenRouterApiKey(apiKey: String) {
        secureEdit {
            putString(KEY_OPENROUTER_API_KEY, apiKey)
        }
    }

    fun loadNvidiaApiKey(): String {
        return getString(KEY_NVIDIA_API_KEY, "") ?: ""
    }

    fun saveNvidiaApiKey(apiKey: String) {
        secureEdit {
            putString(KEY_NVIDIA_API_KEY, apiKey)
        }
    }

    fun loadOllamaUrl(): String {
        return getString(KEY_OLLAMA_URL, "http://localhost:11434") ?: "http://localhost:11434"
    }

    fun saveOllamaUrl(url: String) {
        secureEdit {
            putString(KEY_OLLAMA_URL, url)
        }
    }

    fun loadPresetRuntimeSettings(): PresetRuntimeSettings {
        val payload = getString(KEY_PRESET_RUNTIME_SETTINGS, null) ?: return PresetRuntimeSettings()
        return runCatching {
            json.decodeFromString<PresetRuntimeSettings>(payload).normalizeModelPriorityChains()
        }.getOrDefault(PresetRuntimeSettings())
    }

    fun loadCustomModels(): List<CustomPresetModelDefinition> {
        val payload = getString(KEY_CUSTOM_MODELS, null) ?: return emptyList()
        return try {
            json.decodeFromString<List<CustomPresetModelDefinition>>(payload)
        } catch (error: Exception) {
            throw CustomModelDataUnavailableException(error)
        }
    }

    fun saveCustomModels(models: List<CustomPresetModelDefinition>) {
        withSecureStoreAccess {
            check(prefs.edit().putString(KEY_CUSTOM_MODELS, json.encodeToString(models)).commit()) {
                "Custom-model settings write failed"
            }
        }
    }

    fun savePresetRuntimeSettings(settings: PresetRuntimeSettings) {
        secureEdit {
            putString(
                KEY_PRESET_RUNTIME_SETTINGS,
                json.encodeToString(PresetRuntimeSettings.serializer(), settings.normalizeModelPriorityChains()),
            )
        }
    }

    fun loadPaneFontSizes(): RealtimePaneFontSizes {
        return RealtimePaneFontSizes(
            transcriptionSp = getInt(KEY_TRANSCRIPTION_FONT_SIZE, 16),
            translationSp = getInt(KEY_TRANSLATION_FONT_SIZE, 16),
        )
    }

    fun savePaneFontSizes(fontSizes: RealtimePaneFontSizes) {
        secureEdit {
            putInt(KEY_TRANSCRIPTION_FONT_SIZE, fontSizes.transcriptionSp)
            putInt(KEY_TRANSLATION_FONT_SIZE, fontSizes.translationSp)
        }
    }

    fun loadRealtimeTtsSettings(): RealtimeTtsSettings {
        return RealtimeTtsSettings(
            enabled = getBoolean(KEY_TTS_ENABLED, false),
            speedPercent = getInt(KEY_TTS_SPEED, 100),
            autoSpeed = getBoolean(KEY_TTS_AUTO_SPEED, true),
            volumePercent = getInt(KEY_TTS_VOLUME, 100),
        )
    }

    fun loadUiPreferences(): MobileUiPreferences {
        val payload = getString(KEY_UI_PREFERENCES, null) ?: return MobileUiPreferences()
        return runCatching {
            json.decodeFromString<MobileUiPreferences>(payload)
        }.getOrDefault(MobileUiPreferences())
    }

    fun loadGlobalTtsSettings(): MobileGlobalTtsSettings {
        val payload = getString(KEY_GLOBAL_TTS_SETTINGS, null) ?: return MobileGlobalTtsSettings()
        return runCatching {
            json.decodeFromString<MobileGlobalTtsSettings>(payload)
        }.getOrDefault(MobileGlobalTtsSettings())
    }

    fun saveRealtimeTtsSettings(settings: RealtimeTtsSettings) {
        secureEdit {
            putBoolean(KEY_TTS_ENABLED, settings.enabled)
            putInt(KEY_TTS_SPEED, settings.speedPercent)
            putBoolean(KEY_TTS_AUTO_SPEED, settings.autoSpeed)
            putInt(KEY_TTS_VOLUME, settings.volumePercent)
        }
    }

    fun saveUiPreferences(preferences: MobileUiPreferences) {
        secureEdit {
            putString(KEY_UI_PREFERENCES, json.encodeToString(MobileUiPreferences.serializer(), preferences))
        }
    }

    fun saveGlobalTtsSettings(settings: MobileGlobalTtsSettings) {
        secureEdit {
            putString(KEY_GLOBAL_TTS_SETTINGS, json.encodeToString(MobileGlobalTtsSettings.serializer(), settings))
        }
    }

    fun loadEdgeVoiceCatalog(): CachedEdgeVoiceCatalog? {
        val payload = getString(KEY_EDGE_VOICE_CATALOG, null) ?: return null
        return runCatching {
            json.decodeFromString<CachedEdgeVoiceCatalog>(payload)
        }.getOrNull()
    }

    fun saveEdgeVoiceCatalog(catalog: CachedEdgeVoiceCatalog) {
        secureEdit {
            putString(KEY_EDGE_VOICE_CATALOG, json.encodeToString(CachedEdgeVoiceCatalog.serializer(), catalog))
        }
    }

    companion object {
        private const val KEY_SESSION_CONFIG = "session_config"
        private const val KEY_TRANSLATION_GUMMY_CONFIG = "translation_gummy_config"
        private const val KEY_TRANSLATION_GUMMY_TRANSCRIPTS = "translation_gummy_transcripts"
        private const val LEGACY_KEY_BILINGUAL_RELAY_CONFIG = "bilingual_relay_config"
        private const val LEGACY_KEY_BILINGUAL_RELAY_TRANSCRIPTS = "bilingual_relay_transcripts"
        private const val KEY_GEMINI_API_KEY = "gemini_api_key"
        private const val KEY_GROQ_API_KEY = "groq_api_key"
        private const val KEY_OPENROUTER_API_KEY = "openrouter_api_key"
        private const val KEY_NVIDIA_API_KEY = "nvidia_api_key"
        private const val KEY_OLLAMA_URL = "ollama_url"
        private const val KEY_PRESET_RUNTIME_SETTINGS = "preset_runtime_settings"
        private const val KEY_CUSTOM_MODELS = "custom_models"
        private const val KEY_TRANSCRIPTION_FONT_SIZE = "transcription_font_size"
        private const val KEY_TRANSLATION_FONT_SIZE = "translation_font_size"
        private const val KEY_TTS_ENABLED = "realtime_tts_enabled"
        private const val KEY_TTS_SPEED = "realtime_tts_speed"
        private const val KEY_TTS_AUTO_SPEED = "realtime_tts_auto_speed"
        private const val KEY_TTS_VOLUME = "realtime_tts_volume"
        private const val KEY_GLOBAL_TTS_SETTINGS = "global_tts_settings"
        private const val KEY_EDGE_VOICE_CATALOG = "edge_voice_catalog"
        private const val KEY_UI_PREFERENCES = "ui_preferences"
    }
}

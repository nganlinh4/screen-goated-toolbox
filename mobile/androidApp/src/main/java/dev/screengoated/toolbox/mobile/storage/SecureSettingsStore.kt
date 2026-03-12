@file:Suppress("DEPRECATION")

package dev.screengoated.toolbox.mobile.storage

import android.content.Context
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.RealtimePaneFontSizes
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import kotlinx.serialization.json.Json

class SecureSettingsStore(
    context: Context,
    private val json: Json,
) {
    private val prefs = EncryptedSharedPreferences.create(
        context,
        PREFS_NAME,
        MasterKey.Builder(context)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build(),
        EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
        EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
    )

    fun loadConfig(): LiveSessionConfig {
        val payload = prefs.getString(KEY_SESSION_CONFIG, null) ?: return LiveSessionConfig()
        return runCatching {
            json.decodeFromString<LiveSessionConfig>(payload)
        }.getOrDefault(LiveSessionConfig())
    }

    fun saveConfig(config: LiveSessionConfig) {
        prefs.edit()
            .putString(KEY_SESSION_CONFIG, json.encodeToString(LiveSessionConfig.serializer(), config))
            .apply()
    }

    fun loadApiKey(): String {
        return prefs.getString(KEY_GEMINI_API_KEY, "") ?: ""
    }

    fun saveApiKey(apiKey: String) {
        prefs.edit()
            .putString(KEY_GEMINI_API_KEY, apiKey)
            .apply()
    }

    fun loadCerebrasApiKey(): String {
        return prefs.getString(KEY_CEREBRAS_API_KEY, "") ?: ""
    }

    fun saveCerebrasApiKey(apiKey: String) {
        prefs.edit()
            .putString(KEY_CEREBRAS_API_KEY, apiKey)
            .apply()
    }

    fun loadPaneFontSizes(): RealtimePaneFontSizes {
        return RealtimePaneFontSizes(
            transcriptionSp = prefs.getInt(KEY_TRANSCRIPTION_FONT_SIZE, 16),
            translationSp = prefs.getInt(KEY_TRANSLATION_FONT_SIZE, 16),
        )
    }

    fun savePaneFontSizes(fontSizes: RealtimePaneFontSizes) {
        prefs.edit()
            .putInt(KEY_TRANSCRIPTION_FONT_SIZE, fontSizes.transcriptionSp)
            .putInt(KEY_TRANSLATION_FONT_SIZE, fontSizes.translationSp)
            .apply()
    }

    fun loadRealtimeTtsSettings(): RealtimeTtsSettings {
        return RealtimeTtsSettings(
            enabled = prefs.getBoolean(KEY_TTS_ENABLED, false),
            speedPercent = prefs.getInt(KEY_TTS_SPEED, 100),
            autoSpeed = prefs.getBoolean(KEY_TTS_AUTO_SPEED, true),
            volumePercent = prefs.getInt(KEY_TTS_VOLUME, 100),
        )
    }

    fun loadGlobalTtsSettings(): MobileGlobalTtsSettings {
        val payload = prefs.getString(KEY_GLOBAL_TTS_SETTINGS, null) ?: return MobileGlobalTtsSettings()
        return runCatching {
            json.decodeFromString<MobileGlobalTtsSettings>(payload)
        }.getOrDefault(MobileGlobalTtsSettings())
    }

    fun saveRealtimeTtsSettings(settings: RealtimeTtsSettings) {
        prefs.edit()
            .putBoolean(KEY_TTS_ENABLED, settings.enabled)
            .putInt(KEY_TTS_SPEED, settings.speedPercent)
            .putBoolean(KEY_TTS_AUTO_SPEED, settings.autoSpeed)
            .putInt(KEY_TTS_VOLUME, settings.volumePercent)
            .apply()
    }

    fun saveGlobalTtsSettings(settings: MobileGlobalTtsSettings) {
        prefs.edit()
            .putString(KEY_GLOBAL_TTS_SETTINGS, json.encodeToString(MobileGlobalTtsSettings.serializer(), settings))
            .apply()
    }

    companion object {
        private const val PREFS_NAME = "sgt_mobile_secure"
        private const val KEY_SESSION_CONFIG = "session_config"
        private const val KEY_GEMINI_API_KEY = "gemini_api_key"
        private const val KEY_CEREBRAS_API_KEY = "cerebras_api_key"
        private const val KEY_TRANSCRIPTION_FONT_SIZE = "transcription_font_size"
        private const val KEY_TRANSLATION_FONT_SIZE = "translation_font_size"
        private const val KEY_TTS_ENABLED = "realtime_tts_enabled"
        private const val KEY_TTS_SPEED = "realtime_tts_speed"
        private const val KEY_TTS_AUTO_SPEED = "realtime_tts_auto_speed"
        private const val KEY_TTS_VOLUME = "realtime_tts_volume"
        private const val KEY_GLOBAL_TTS_SETTINGS = "global_tts_settings"
    }
}

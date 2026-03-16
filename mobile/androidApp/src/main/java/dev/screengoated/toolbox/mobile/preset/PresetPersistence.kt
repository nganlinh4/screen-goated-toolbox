package dev.screengoated.toolbox.mobile.preset

import android.content.Context
import kotlinx.serialization.json.Json
import java.io.File

interface PresetOverrideStore {
    fun load(): StoredPresetOverrides

    fun save(overrides: StoredPresetOverrides)
}

class PresetPersistence(
    context: Context,
    private val json: Json,
) : PresetOverrideStore {
    private val configFile = File(context.filesDir, "preset_overrides.json")

    override fun load(): StoredPresetOverrides {
        return try {
            if (configFile.exists()) {
                json.decodeFromString<StoredPresetOverrides>(configFile.readText())
            } else {
                StoredPresetOverrides()
            }
        } catch (_: Exception) {
            StoredPresetOverrides()
        }
    }

    override fun save(overrides: StoredPresetOverrides) {
        try {
            configFile.writeText(json.encodeToString(StoredPresetOverrides.serializer(), overrides))
        } catch (_: Exception) {
            // Ignore write failures and keep in-memory state alive.
        }
    }
}

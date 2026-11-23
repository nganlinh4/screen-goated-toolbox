package dev.screengoated.toolbox.mobile.preset

import android.content.Context
import kotlinx.serialization.json.Json
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.NoSuchFileException
import java.nio.file.Path
import java.nio.file.StandardCopyOption
import java.util.concurrent.ConcurrentHashMap

interface PresetOverrideStore {
    fun load(): StoredPresetOverrides
    fun save(overrides: StoredPresetOverrides)
}

class PresetPersistence internal constructor(
    private val configFile: File,
    private val json: Json,
    private val replace: (Path, Path) -> Unit = { source, target ->
        Files.move(source, target, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
    },
) : PresetOverrideStore {
    constructor(context: Context, json: Json) : this(File(context.filesDir, "preset_overrides.json"), json)

    private val path = configFile.toPath().toAbsolutePath().normalize()
    private val lock = locks.computeIfAbsent(path.toString()) { Any() }
    private val strictJson = Json(json) { ignoreUnknownKeys = false }
    private var loaded = false
    private var snapshot: ByteArray? = null

    override fun load(): StoredPresetOverrides = synchronized(lock) {
        loaded = false
        val data = readCurrent()
        val result = data?.let {
            val text = Charsets.UTF_8.newDecoder()
                .onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT)
                .decode(ByteBuffer.wrap(it)).toString()
            strictJson.decodeFromString<StoredPresetOverrides>(text)
        }
            ?: StoredPresetOverrides()
        if (result.version != 1) throw IOException("Unsupported preset storage version")
        snapshot = data
        loaded = true
        result
    }

    override fun save(overrides: StoredPresetOverrides) = synchronized(lock) {
        if (!loaded) throw IOException("Preset storage has not been read successfully")
        val current = readCurrent()
        if (!(current == null && snapshot == null) && (current == null || snapshot == null || !current.contentEquals(snapshot))) {
            throw IOException("Preset storage changed since it was read")
        }
        val bytes = json.encodeToString(StoredPresetOverrides.serializer(), overrides).toByteArray(Charsets.UTF_8)
        val temp = Files.createTempFile(path.parent, ".preset-overrides-", ".tmp")
        try {
            FileOutputStream(temp.toFile()).use { file ->
                file.write(bytes)
                file.fd.sync()
            }
            replace(temp, path)
            snapshot = bytes
        } finally {
            Files.deleteIfExists(temp)
        }
    }

    private fun readCurrent(): ByteArray? = try {
        if (Files.isSymbolicLink(path)) throw IOException("Preset storage cannot be a symbolic link")
        Files.readAllBytes(path)
    } catch (error: NoSuchFileException) {
        if (Files.exists(path, LinkOption.NOFOLLOW_LINKS)) throw error
        null
    }

    private companion object {
        val locks = ConcurrentHashMap<String, Any>()
    }
}

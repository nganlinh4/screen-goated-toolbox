package dev.screengoated.toolbox.mobile.phonecontrol.provider

import android.content.Context
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.io.File
import java.security.MessageDigest
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

internal data class PhoneControlArtifact(
    val id: String,
    val bytes: ByteArray,
    val mimeType: String,
    val name: String?,
    val createdAtMs: Long,
) {
    val sha256: String
        get() = bytes.sha256()

    fun info(): JsonObject = buildJsonObject {
        put("id", id)
        put("byte_count", bytes.size)
        put("mime_type", mimeType)
        put("name", name.orEmpty())
        put("sha256", sha256)
        put("created_at_ms", createdAtMs)
    }
}

internal class PhoneControlArtifactStore(
    context: Context,
) {
    private val cacheDir = File(context.cacheDir, "phone-control-artifacts").apply { mkdirs() }
    private val sequence = AtomicLong(0L)
    private val memory = ConcurrentHashMap<String, PhoneControlArtifact>()

    fun put(
        bytes: ByteArray,
        mimeType: String,
        name: String? = null,
    ): PhoneControlArtifact {
        val number = sequence.incrementAndGet()
        val artifact = PhoneControlArtifact(
            id = "phone-artifact-$number-${bytes.sha256().take(12)}",
            bytes = bytes.copyOf(),
            mimeType = mimeType,
            name = name,
            createdAtMs = android.os.SystemClock.elapsedRealtime(),
        )
        memory[artifact.id] = artifact
        File(cacheDir, artifact.id).writeBytes(artifact.bytes)
        return artifact
    }

    fun get(id: String): PhoneControlArtifact? = memory[id]

    fun remove(id: String) {
        memory.remove(id)
        File(cacheDir, id).delete()
    }

    fun clear() {
        memory.clear()
        cacheDir.listFiles()?.forEach(File::delete)
    }
}

internal fun ByteArray.sha256(): String = MessageDigest.getInstance("SHA-256")
    .digest(this)
    .joinToString("") { "%02x".format(it) }

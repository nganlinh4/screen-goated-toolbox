package dev.screengoated.toolbox.mobile.phonecontrol.provider

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.security.MessageDigest
import java.util.UUID

internal class PhoneControlArtifact(
    val id: String,
    bytes: ByteArray,
    val mimeType: String,
    val name: String?,
    val createdAtMs: Long,
) {
    private val content = bytes.copyOf()
    val bytes: ByteArray get() = content.copyOf()
    val sha256: String = content.sha256()

    fun info(): JsonObject = buildJsonObject {
        put("id", id)
        put("byte_count", content.size)
        put("mime_type", mimeType)
        put("name", name.orEmpty())
        put("sha256", sha256)
        put("created_at_ms", createdAtMs)
    }
}

internal class PhoneControlArtifactStore(
    private val maximumBytes: Int = MAXIMUM_ARTIFACT_BYTES,
    private val maximumEntries: Int = MAXIMUM_ARTIFACT_ENTRIES,
    private val nowMillis: () -> Long = android.os.SystemClock::elapsedRealtime,
) : AutoCloseable {
    private val memory = HashMap<String, PhoneControlArtifact>()
    private var retainedBytes = 0L
    private var closed = false

    init {
        require(maximumBytes > 0 && maximumEntries > 0)
    }

    @Synchronized
    fun put(
        bytes: ByteArray,
        mimeType: String,
        name: String? = null,
    ): PhoneControlArtifact {
        if (closed) throw PhoneControlArtifactStoreException(ArtifactStoreRejection.RETIRED)
        val createdAtMs = nowMillis()
        if (closed) throw PhoneControlArtifactStoreException(ArtifactStoreRejection.RETIRED)
        if (bytes.size > maximumBytes || memory.size >= maximumEntries) {
            throw PhoneControlArtifactStoreException(ArtifactStoreRejection.LIMIT)
        }
        val required = bytes.size.toLong() + metadataBytes(mimeType) + metadataBytes(name.orEmpty())
        if (memory.size >= maximumEntries || required > maximumBytes || retainedBytes > maximumBytes - required) {
            throw PhoneControlArtifactStoreException(ArtifactStoreRejection.LIMIT)
        }
        val artifact = PhoneControlArtifact(
            id = "phone-artifact-${UUID.randomUUID()}",
            bytes = bytes,
            mimeType = mimeType,
            name = name,
            createdAtMs = createdAtMs,
        )
        memory[artifact.id] = artifact
        retainedBytes += required
        return artifact
    }

    @Synchronized
    fun get(id: String): PhoneControlArtifact? = if (closed) null else memory[id]

    @Synchronized
    override fun close() {
        closed = true
        memory.clear()
        retainedBytes = 0
    }

    private fun metadataBytes(value: String): Long {
        var bytes = 0L
        var index = 0
        while (index < value.length) {
            val character = value[index++]
            bytes += when {
                character.isHighSurrogate() -> {
                    if (index >= value.length || !value[index++].isLowSurrogate()) {
                        throw PhoneControlArtifactStoreException(ArtifactStoreRejection.INVALID_METADATA)
                    }
                    4
                }
                character.isLowSurrogate() -> throw PhoneControlArtifactStoreException(ArtifactStoreRejection.INVALID_METADATA)
                character.code < 0x80 -> 1
                character.code < 0x800 -> 2
                else -> 3
            }
            if (bytes > maximumBytes) throw PhoneControlArtifactStoreException(ArtifactStoreRejection.LIMIT)
        }
        return bytes
    }

    companion object {
        const val MAXIMUM_ARTIFACT_BYTES = 32 * 1024 * 1024
        const val MAXIMUM_ARTIFACT_ENTRIES = 128
    }
}

internal enum class ArtifactStoreRejection(val code: String) {
    LIMIT("artifact_limit_reached"),
    RETIRED("artifact_store_closed"),
    INVALID_METADATA("artifact_metadata_invalid"),
}

internal class PhoneControlArtifactStoreException(val rejection: ArtifactStoreRejection) :
    IllegalStateException(rejection.code)

internal fun ByteArray.sha256(): String = MessageDigest.getInstance("SHA-256")
    .digest(this)
    .joinToString("") { "%02x".format(it) }

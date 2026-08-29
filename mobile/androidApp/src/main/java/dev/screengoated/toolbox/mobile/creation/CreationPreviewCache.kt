package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.io.InputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest
import java.util.UUID

internal class CreationPreviewCache {
    private val locks = Array(32) { Any() }

    fun materialize(
        directory: File,
        key: String,
        extension: String,
        maximumBytes: Long,
        reusable: Boolean,
        openInput: () -> InputStream,
        validate: (File) -> Unit,
    ): File {
        val lock = locks[(key.hashCode() and Int.MAX_VALUE) % locks.size]
        return synchronized(lock) {
            directory.mkdirs()
            val target = File(directory, "$key.$extension")
            if (reusable && isCreationRegularFileConfined(directory, target)) {
                validate(target)
                return@synchronized target
            }
            val temporary = File(directory, ".${target.name}.tmp-${UUID.randomUUID()}")
            try {
                check(temporary.createNewFile()) { "Could not reserve preview cache" }
                openInput().use { copyCreationInputBounded(it, temporary, maximumBytes) }
                validate(temporary)
                runCatching {
                    Files.move(
                        temporary.toPath(),
                        target.toPath(),
                        StandardCopyOption.ATOMIC_MOVE,
                        StandardCopyOption.REPLACE_EXISTING,
                    )
                }.getOrElse {
                    Files.move(
                        temporary.toPath(),
                        target.toPath(),
                        StandardCopyOption.REPLACE_EXISTING,
                    )
                }
                require(isCreationRegularFileConfined(directory, target)) {
                    "Preview cache is unavailable"
                }
                validate(target)
                target
            } finally {
                temporary.delete()
            }
        }
    }
}

internal fun creationPreviewCacheKey(
    sourceIdentity: String,
    extension: String,
    sourceVersion: String?,
): String {
    val canonical = "$sourceIdentity\u0000$extension\u0000${sourceVersion.orEmpty()}"
    return MessageDigest.getInstance("SHA-256")
        .digest(canonical.encodeToByteArray())
        .joinToString("") { byte -> "%02x".format(byte) }
}

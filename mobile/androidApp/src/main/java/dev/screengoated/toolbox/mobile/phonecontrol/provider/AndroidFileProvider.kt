package dev.screengoated.toolbox.mobile.phonecontrol.provider

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.io.File
import java.nio.charset.CodingErrorAction
import java.nio.file.Files

internal class AndroidFileProvider(
    private val findArtifact: (String) -> PhoneControlArtifact?,
    private val storeArtifact: (ByteArray, String, String?) -> PhoneControlArtifact,
) {
    constructor(artifacts: PhoneControlArtifactStore) : this(artifacts::get, artifacts::put)

    internal constructor(findArtifact: (String) -> PhoneControlArtifact?) : this(
        findArtifact,
        { _, _, _ -> error("Artifact writes are not configured") },
    )

    fun list(
        path: String,
        kind: String?,
        extensions: Set<String>,
        sortBy: String,
        descending: Boolean,
        limit: Int,
    ): AndroidProviderResult {
        val directory = resolve(path)
            ?: return failure("invalid_path", "The path could not be resolved.")
        if (!directory.isDirectory || !directory.canRead()) {
            return failure("path_unavailable", "The directory is not readable.")
        }
        val normalizedExtensions = extensions.map { it.trim().trimStart('.').lowercase() }.toSet()
        var files = directory.listFiles().orEmpty().asSequence().filter { file ->
            when (kind) {
                "file" -> file.isFile
                "directory" -> file.isDirectory
                null, "any" -> true
                else -> false
            }
        }.filter { file ->
            normalizedExtensions.isEmpty() ||
                file.isDirectory || file.extension.lowercase() in normalizedExtensions
        }.toList()
        val comparator = when (sortBy) {
            "name" -> compareBy<File> { it.name.lowercase() }
            "size" -> compareBy(File::length)
            "created" -> compareBy<File> { creationTime(it) }
            else -> compareBy(File::lastModified)
        }
        files = files.sortedWith(if (descending) comparator.reversed() else comparator)
            .take(limit.coerceIn(1, MAX_LIST_ITEMS))
        return AndroidProviderResult.Success(
            buildJsonObject {
                put("path", directory.absolutePath)
                put("count", files.size)
                put(
                    "items",
                    buildJsonArray {
                        files.forEach { file ->
                            add(
                                buildJsonObject {
                                    put("name", file.name)
                                    put("path", file.absolutePath)
                                    put("kind", if (file.isDirectory) "directory" else "file")
                                    put("size", file.length())
                                    put("modified_ms", file.lastModified())
                                    put("readable", file.canRead())
                                    put("writable", file.canWrite())
                                },
                            )
                        }
                    },
                )
            },
        )
    }

    fun readText(path: String, expectedSha256: String?, maxChars: Int): AndroidProviderResult {
        val file = resolve(path)
            ?: return failure("invalid_path", "The path could not be resolved.")
        if (!file.isFile || !file.canRead()) return failure("path_unavailable", "The file is not readable.")
        if (file.length() > MAX_TEXT_BYTES) return failure("file_too_large", "The file exceeds the bounded text limit.")
        val bytes = runCatching { file.readBytes() }.getOrElse {
            return failure("read_failed", it.message ?: "The file could not be read.")
        }
        val sha = bytes.sha256()
        if (expectedSha256 != null && !sha.equals(expectedSha256, ignoreCase = true)) {
            return failure("hash_mismatch", "The file changed since the supplied hash.")
        }
        val text = decodeUtf8(bytes) ?: return failure("not_utf8", "The file is not valid UTF-8 text.")
        val bounded = text.take(maxChars.coerceIn(1, MAX_TEXT_CHARS))
        val artifact = storeArtifact(bytes, "text/plain; charset=utf-8", file.name)
        return AndroidProviderResult.Success(
            buildJsonObject {
                put("path", file.absolutePath)
                put("sha256", sha)
                put("text", bounded)
                put("characters", text.length)
                put("truncated", bounded.length < text.length)
                put("artifact_id", artifact.id)
            },
        )
    }

    fun exactReplace(
        path: String,
        expectedSha256: String,
        replacements: List<ExactReplacement>,
    ): AndroidProviderResult {
        if (replacements.isEmpty()) return failure("invalid_request", "At least one replacement is required.")
        val file = resolve(path)
            ?: return failure("invalid_path", "The path could not be resolved.")
        return AndroidFileMutationCoordinator.withExclusivePath(file) {
            exactReplaceLocked(file, expectedSha256, replacements)
        }
    }

    fun saveArtifact(id: String, path: String, overwrite: Boolean): AndroidProviderResult {
        val artifact = findArtifact(id) ?: return failure("artifact_not_found", "The artifact ID is unknown.")
        val file = resolve(path)
            ?: return failure("invalid_path", "The path could not be resolved.")
        return AndroidFileMutationCoordinator.withExclusivePath(file) {
            saveArtifactLocked(artifact, file, overwrite)
        }
    }

    private fun exactReplaceLocked(
        file: File,
        expectedSha256: String,
        replacements: List<ExactReplacement>,
    ): AndroidProviderResult {
        if (!file.isFile || !file.canRead() || !file.canWrite()) {
            return failure("path_unavailable", "The file is not readable and writable.")
        }
        val original = runCatching { file.readBytes() }.getOrElse {
            return failure("read_failed", it.message ?: "The file could not be read.")
        }
        val beforeSha256 = original.sha256()
        if (!beforeSha256.equals(expectedSha256, ignoreCase = true)) {
            return failure(EXACT_FILE_CHANGED_CODE, "The file changed since it was read.")
        }
        var text = decodeUtf8(original) ?: return failure("not_utf8", "The file is not valid UTF-8 text.")
        for (replacement in replacements) {
            val actualCount = countExact(text, replacement.oldText)
            if (actualCount != replacement.expectedCount) {
                return failure(
                    "replacement_count_mismatch",
                    "An exact replacement count did not match the current file.",
                )
            }
            text = text.replace(replacement.oldText, replacement.newText)
        }
        val updated = text.toByteArray(Charsets.UTF_8)
        var staged: java.nio.file.Path? = null
        return try {
            val stagedPath = AndroidFileMutationCoordinator.stageSibling(file, updated)
            staged = stagedPath
            when (
                AndroidFileMutationCoordinator.replaceIfExpected(
                    file,
                    stagedPath,
                    expectedSha256,
                )
            ) {
                is ExpectedFileCommit.Changed -> failure(
                    EXACT_FILE_CHANGED_CODE,
                    "The file changed before the atomic replacement.",
                )
                is ExpectedFileCommit.Replaced -> {
                    staged = null
                    val verified = file.readBytes()
                    check(verified.contentEquals(updated)) { "Post-write verification failed" }
                    AndroidProviderResult.Success(
                        buildJsonObject {
                            put("path", file.absolutePath)
                            put("before_sha256", beforeSha256)
                            put("sha256", verified.sha256())
                            put("replacement_count", replacements.sumOf(ExactReplacement::expectedCount))
                        },
                        effectMayHaveOccurred = true,
                        effectVerified = true,
                    )
                }
            }
        } catch (error: Throwable) {
            AndroidProviderResult.Failure(
                "write_failed",
                error.message ?: "The atomic file update failed.",
                retryable = true,
            )
        } finally {
            staged?.let { runCatching { Files.deleteIfExists(it) } }
        }
    }

    private fun saveArtifactLocked(
        artifact: PhoneControlArtifact,
        file: File,
        overwrite: Boolean,
    ): AndroidProviderResult {
        if (file.isDirectory) {
            return failure("path_unavailable", "The destination path is a directory.")
        }
        val parent = file.parentFile
        if (parent != null && !parent.isDirectory && !parent.mkdirs() && !parent.isDirectory) {
            return failure("save_failed", "The destination directory could not be created.")
        }
        var staged: java.nio.file.Path? = null
        return try {
            if (overwrite) {
                val stagedPath = AndroidFileMutationCoordinator.stageSibling(file, artifact.bytes)
                staged = stagedPath
                AndroidFileMutationCoordinator.replace(file, stagedPath)
                staged = null
            } else {
                AndroidFileMutationCoordinator.createNew(file, artifact.bytes)
            }
            check(file.readBytes().contentEquals(artifact.bytes)) { "Saved bytes did not verify" }
            AndroidProviderResult.Success(
                buildJsonObject {
                    put("path", file.absolutePath)
                    put("sha256", artifact.sha256)
                    put("bytes", artifact.bytes.size)
                },
                effectMayHaveOccurred = true,
                effectVerified = true,
            )
        } catch (error: Throwable) {
            if (!overwrite && error.isExistingPathConflict()) {
                failure("path_exists", "The destination already exists.")
            } else {
                AndroidProviderResult.Failure(
                    "save_failed",
                    error.message ?: "The artifact could not be saved.",
                )
            }
        } finally {
            staged?.let { runCatching { Files.deleteIfExists(it) } }
        }
    }

    private fun resolve(path: String): File? = runCatching {
        File(path.trim()).canonicalFile
    }.getOrNull()

    private fun failure(code: String, message: String) = AndroidProviderResult.Failure(code, message)
}

internal data class ExactReplacement(
    val oldText: String,
    val newText: String,
    val expectedCount: Int,
)

private fun decodeUtf8(bytes: ByteArray): String? = runCatching {
    Charsets.UTF_8.newDecoder()
        .onMalformedInput(CodingErrorAction.REPORT)
        .onUnmappableCharacter(CodingErrorAction.REPORT)
        .decode(java.nio.ByteBuffer.wrap(bytes))
        .toString()
}.getOrNull()

private fun countExact(text: String, needle: String): Int {
    if (needle.isEmpty()) return 0
    var count = 0
    var offset = 0
    while (true) {
        val found = text.indexOf(needle, offset)
        if (found < 0) return count
        count += 1
        offset = found + needle.length
    }
}

private fun creationTime(file: File): Long = runCatching {
    Files.readAttributes(file.toPath(), java.nio.file.attribute.BasicFileAttributes::class.java)
        .creationTime()
        .toMillis()
}.getOrDefault(file.lastModified())

private const val MAX_LIST_ITEMS = 2_000
private const val MAX_TEXT_BYTES = 8L * 1024L * 1024L
private const val MAX_TEXT_CHARS = 64_000

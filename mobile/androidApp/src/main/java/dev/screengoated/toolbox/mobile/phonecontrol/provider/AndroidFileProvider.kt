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
        val visibleEntries = directory.listFiles().orEmpty()
        val matchingFiles = visibleEntries.asSequence().filter { file ->
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
        val files = matchingFiles
            .sortedWith(if (descending) comparator.reversed() else comparator)
            .take(limit.coerceIn(1, MAX_LIST_ITEMS))
        return AndroidProviderResult.Success(
            buildJsonObject {
                put("path", directory.absolutePath)
                put("count", files.size)
                put("total_entry_count", visibleEntries.size)
                put("matched_entry_count", matchingFiles.size)
                put("listing_complete", matchingFiles.size <= limit)
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
        targetLease: FileMutationTargetLease,
    ): AndroidProviderResult {
        if (replacements.isEmpty()) return failure("invalid_request", "At least one replacement is required.")
        val file = resolve(path)
            ?: return failure("invalid_path", "The path could not be resolved.")
        return AndroidFileMutationCoordinator.withExclusivePath(file) {
            targetIdentityFailure(file, targetLease)?.let { return@withExclusivePath it }
            exactReplaceLocked(file, expectedSha256, replacements)
        }
    }

    fun structuralPreflight(
        path: String,
        expectedSha256: String,
        replacements: List<ExactReplacement>,
        suppliedToken: String?,
    ): AndroidProviderResult {
        if (replacements.isEmpty()) return failure("invalid_request", "At least one replacement is required.")
        val file = resolve(path)
            ?: return failure("invalid_path", "The path could not be resolved.")
        return AndroidFileMutationCoordinator.withExclusivePath(file) {
            when (
                val prepared = prepareFileEditLocked(file, expectedSha256) { original ->
                    ExactTextEditPlanner.planStructural(
                        file,
                        original,
                        replacements,
                        suppliedToken,
                    )
                }
            ) {
                is PreparedFileEditResult.Failure -> prepared.result
                is PreparedFileEditResult.Ready -> AndroidProviderResult.Success(
                    buildJsonObject {
                        editEvidence(prepared.edit, editScope = "structure")
                            .forEach { (key, value) -> put(key, value) }
                        put("ready_for_request_contract_check", true)
                        put("original_unchanged", true)
                    },
                )
            }
        }
    }

    fun commitStructuralAfterAuthorization(
        path: String,
        expectedSha256: String,
        replacements: List<ExactReplacement>,
        suppliedToken: String,
        targetLease: FileMutationTargetLease,
    ): AndroidProviderResult {
        if (replacements.isEmpty()) return failure("invalid_request", "At least one replacement is required.")
        val file = resolve(path)
            ?: return failure("invalid_path", "The path could not be resolved.")
        return AndroidFileMutationCoordinator.withExclusivePath(file) {
            targetIdentityFailure(file, targetLease)?.let { return@withExclusivePath it }
            when (
                val prepared = prepareFileEditLocked(file, expectedSha256) { original ->
                    ExactTextEditPlanner.planStructural(
                        file,
                        original,
                        replacements,
                        suppliedToken,
                    )
                }
            ) {
                is PreparedFileEditResult.Failure -> prepared.result
                is PreparedFileEditResult.Ready ->
                    commitPreparedEdit(prepared.edit, editScope = "structure")
            }
        }
    }

    fun saveArtifact(
        id: String,
        path: String,
        overwrite: Boolean,
        targetLease: FileMutationTargetLease,
    ): AndroidProviderResult {
        val artifact = findArtifact(id) ?: return failure("artifact_not_found", "The artifact ID is unknown.")
        val file = resolve(path)
            ?: return failure("invalid_path", "The path could not be resolved.")
        return AndroidFileMutationCoordinator.withExclusivePath(file) {
            saveArtifactLocked(artifact, file, overwrite, targetLease)
        }
    }

    private fun exactReplaceLocked(
        file: File,
        expectedSha256: String,
        replacements: List<ExactReplacement>,
    ): AndroidProviderResult = when (
        val prepared = prepareFileEditLocked(file, expectedSha256) { original ->
            ExactTextEditPlanner.planOrdinary(file, original, replacements)
        }
    ) {
        is PreparedFileEditResult.Failure -> prepared.result
        is PreparedFileEditResult.Ready ->
            commitPreparedEdit(prepared.edit, editScope = "content")
    }

    private fun prepareFileEditLocked(
        file: File,
        expectedSha256: String,
        planner: (String) -> TextEditPlan,
    ): PreparedFileEditResult {
        if (!file.isFile || !file.canRead() || !file.canWrite()) {
            return preparedFailure(failure("path_unavailable", "The file is not readable and writable."))
        }
        if (file.length() > MAX_TEXT_BYTES) {
            return preparedFailure(
                failure("ERR_TEXT_FILE_TOO_LARGE", "The file exceeds the bounded text limit."),
            )
        }
        val original = runCatching { file.readBytes() }.getOrElse {
            return preparedFailure(
                failure("read_failed", it.message ?: "The file could not be read."),
            )
        }
        if (original.size.toLong() > MAX_TEXT_BYTES) {
            return preparedFailure(
                failure("ERR_TEXT_FILE_TOO_LARGE", "The file exceeds the bounded text limit."),
            )
        }
        val beforeSha256 = original.sha256()
        if (!beforeSha256.equals(expectedSha256, ignoreCase = true)) {
            return preparedFailure(
                failure(EXACT_FILE_CHANGED_CODE, "The file changed since it was read."),
            )
        }
        val decoded = decodeUtf8(original)
            ?: return preparedFailure(failure("not_utf8", "The file is not valid UTF-8 text."))
        val hasBom = decoded.startsWith(UTF8_BOM)
        val text = if (hasBom) decoded.drop(1) else decoded
        val edit = when (val plan = planner(text)) {
            is TextEditPlan.Ready -> plan.edit
            is TextEditPlan.Rejected -> {
                return preparedFailure(
                    AndroidProviderResult.Failure(
                        code = plan.code,
                        message = plan.message,
                        retryable = true,
                        data = buildJsonObject {
                            plan.data.forEach { (key, value) -> put(key, value) }
                            put("path", file.absolutePath)
                            put("before_sha256", beforeSha256)
                            put("original_unchanged", true)
                        },
                    ),
                )
            }
        }
        val editedBody = edit.text.toByteArray(Charsets.UTF_8)
        val updated = if (hasBom) UTF8_BOM_BYTES + editedBody else editedBody
        if (updated.size.toLong() > MAX_TEXT_BYTES) {
            return preparedFailure(
                AndroidProviderResult.Failure(
                    code = "ERR_TEXT_FILE_TOO_LARGE",
                    message = "The edited file exceeds the bounded text limit.",
                    retryable = true,
                    data = buildJsonObject {
                        put("path", file.absolutePath)
                        put("before_sha256", beforeSha256)
                        put("byte_count", updated.size)
                        put("max_byte_count", MAX_TEXT_BYTES)
                        put("original_unchanged", true)
                    },
                ),
            )
        }
        return PreparedFileEditResult.Ready(
            PreparedFileEdit(
                file = file,
                original = original,
                beforeSha256 = beforeSha256,
                hasBom = hasBom,
                edit = edit,
                updated = updated,
            ),
        )
    }

    private fun commitPreparedEdit(
        prepared: PreparedFileEdit,
        editScope: String,
    ): AndroidProviderResult {
        var staged: java.nio.file.Path? = null
        return try {
            val stagedPath = AndroidFileMutationCoordinator.stageSibling(
                prepared.file,
                prepared.updated,
            )
            staged = stagedPath
            when (
                AndroidFileMutationCoordinator.replaceIfExpected(
                    prepared.file,
                    stagedPath,
                    prepared.beforeSha256,
                )
            ) {
                is ExpectedFileCommit.Changed -> failure(
                    EXACT_FILE_CHANGED_CODE,
                    "The file changed before the atomic replacement.",
                )
                is ExpectedFileCommit.Replaced -> {
                    staged = null
                    val verified = prepared.file.readBytes()
                    check(verified.contentEquals(prepared.updated)) { "Post-write verification failed" }
                    AndroidProviderResult.Success(
                        buildJsonObject {
                            editEvidence(prepared, editScope)
                                .forEach { (key, value) -> put(key, value) }
                            put("sha256", verified.sha256())
                            put("original_unchanged", false)
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

    private fun editEvidence(
        prepared: PreparedFileEdit,
        editScope: String,
    ): JsonObject = buildJsonObject {
        put("path", prepared.file.absolutePath)
        put("before_sha256", prepared.beforeSha256)
        put("before_byte_count", prepared.original.size)
        put("byte_count", prepared.updated.size)
        put("char_count", prepared.edit.text.length)
        put("replacement_count", prepared.edit.replacementCount)
        put("replacements_applied", prepared.edit.replacementCount)
        put("replacement_groups", prepared.edit.replacementGroups)
        put("requested_replacement_groups", prepared.edit.requestedReplacementGroups)
        put("formula_cells_auto_preserved", prepared.edit.formulaCellsAutoPreserved)
        put(
            "formula_replacement_groups_rewritten",
            prepared.edit.formulaReplacementGroupsRewritten,
        )
        put("formula_only_groups_omitted", prepared.edit.formulaOnlyGroupsOmitted)
        put("trailing_empty_fields_omitted", prepared.edit.trailingEmptyFieldsOmitted)
        put("trailing_value_fields_repaired", prepared.edit.trailingValueFieldsRepaired)
        prepared.edit.structure?.let { put("structure", it) }
        put("edit_scope", editScope)
        put("encoding", if (prepared.hasBom) "utf-8-bom" else "utf-8")
        put("atomic", true)
    }

    private fun preparedFailure(
        result: AndroidProviderResult,
    ): PreparedFileEditResult.Failure = PreparedFileEditResult.Failure(result)

    private fun saveArtifactLocked(
        artifact: PhoneControlArtifact,
        file: File,
        overwrite: Boolean,
        targetLease: FileMutationTargetLease,
    ): AndroidProviderResult {
        if (
            !targetLease.existedBefore &&
            !overwrite &&
            file.absolutePath == targetLease.canonicalPath &&
            file.isFile
        ) {
            return failure("path_exists", "The destination already exists.")
        }
        targetIdentityFailure(file, targetLease)?.let { return it }
        if (file.isDirectory) {
            return failure("path_unavailable", "The destination path is a directory.")
        }
        val parent = file.parentFile
        if (parent != null && !parent.isDirectory && !parent.mkdirs() && !parent.isDirectory) {
            return failure("save_failed", "The destination directory could not be created.")
        }
        var staged: java.nio.file.Path? = null
        return try {
            if (targetLease.existedBefore && !overwrite) {
                return failure("path_exists", "The destination already exists.")
            }
            if (targetLease.existedBefore) {
                targetLeaseFailure(file, targetLease)?.let { return it }
                val stagedPath = AndroidFileMutationCoordinator.stageSibling(file, artifact.bytes)
                staged = stagedPath
                targetLeaseFailure(file, targetLease)?.let { return it }
                when (
                    AndroidFileMutationCoordinator.replaceIfExpected(
                        file,
                        stagedPath,
                        checkNotNull(targetLease.expectedSha256),
                    )
                ) {
                    is ExpectedFileCommit.Changed -> return targetLeaseChanged()
                    is ExpectedFileCommit.Replaced -> staged = null
                }
            } else {
                targetLeaseFailure(file, targetLease)?.let { return it }
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

    private fun targetLeaseFailure(
        file: File,
        lease: FileMutationTargetLease,
    ): AndroidProviderResult.Failure? =
        if (AndroidFileMutationCoordinator.targetMatchesLease(file, lease)) {
            null
        } else {
            targetLeaseChanged()
        }

    private fun targetIdentityFailure(
        file: File,
        lease: FileMutationTargetLease,
    ): AndroidProviderResult.Failure? =
        if (AndroidFileMutationCoordinator.targetIdentityMatchesLease(file, lease)) {
            null
        } else {
            targetLeaseChanged()
        }

    private fun targetLeaseChanged() = AndroidProviderResult.Failure(
        code = "ERR_FILE_TARGET_LEASE_CHANGED",
        message = "The authorized file target changed before commit.",
        retryable = true,
        data = buildJsonObject {
            put("original_unchanged", true)
        },
    )

    private fun resolve(path: String): File? = runCatching {
        File(path.trim()).canonicalFile
    }.getOrNull()

    private fun failure(code: String, message: String) = AndroidProviderResult.Failure(code, message)
}

private data class PreparedFileEdit(
    val file: File,
    val original: ByteArray,
    val beforeSha256: String,
    val hasBom: Boolean,
    val edit: PreparedTextEdit,
    val updated: ByteArray,
)

private sealed interface PreparedFileEditResult {
    data class Ready(val edit: PreparedFileEdit) : PreparedFileEditResult
    data class Failure(val result: AndroidProviderResult) : PreparedFileEditResult
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

private fun creationTime(file: File): Long = runCatching {
    Files.readAttributes(file.toPath(), java.nio.file.attribute.BasicFileAttributes::class.java)
        .creationTime()
        .toMillis()
}.getOrDefault(file.lastModified())

private const val MAX_LIST_ITEMS = 2_000
private const val MAX_TEXT_BYTES = 8L * 1024L * 1024L
private const val MAX_TEXT_CHARS = 64_000
private const val UTF8_BOM = '\uFEFF'
private val UTF8_BOM_BYTES = byteArrayOf(0xEF.toByte(), 0xBB.toByte(), 0xBF.toByte())

package dev.screengoated.toolbox.mobile.phonecontrol.provider

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import java.io.File
import java.nio.charset.CodingErrorAction
import java.nio.file.Files
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.AndroidFileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileTransactionIo
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.PreparedFileTarget
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FilePublication
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.TargetPreparationException
import dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio.FileSnapshotLimitExceeded
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty

internal class AndroidFileProvider(
    private val findArtifact: (String) -> PhoneControlArtifact?,
    private val storeArtifact: (ByteArray, String, String?) -> PhoneControlArtifact,
    private val fileIo: FileTransactionIo = AndroidFileTransactionIo,
) {
    constructor(artifacts: PhoneControlArtifactStore, fileIo: FileTransactionIo = AndroidFileTransactionIo) :
        this(artifacts::get, artifacts::put, fileIo)

    internal constructor(findArtifact: (String) -> PhoneControlArtifact?, fileIo: FileTransactionIo = AndroidFileTransactionIo) : this(
        findArtifact, { _, _, _ -> error("Artifact writes are not configured") }, fileIo,
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
        val bytes = runCatching { readArtifactFileBounded(file, MAX_TEXT_BYTES.toInt()) }.getOrElse {
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

    suspend fun withTarget(
        path: String, operation: suspend (PreparedFileTarget) -> AndroidProviderResult,
    ): AndroidProviderResult {
        val target = try { prepareTarget(path) } catch (error: Throwable) { return preparationFailure(error) }
        return target.use { operation(it) }
    }

    fun prepareTarget(path: String): PreparedFileTarget =
        PreparedFileTarget.prepare(path, MAX_TEXT_BYTES.toInt(), fileIo, requireExisting = true)

    fun prepareArtifactSave(id: String, path: String, overwrite: Boolean): PreparedArtifactSave {
        val artifact = findArtifact(id) ?: throw TargetPreparationException("artifact_not_found", "The artifact ID is unknown")
        val target = PreparedFileTarget.prepare(path, PhoneControlArtifactStore.MAXIMUM_ARTIFACT_BYTES, fileIo, overwrite = overwrite)
        return try {
            PreparedArtifactSave(artifact, target, overwrite) { findArtifact(id) === artifact }
        } catch (error: Throwable) { target.close(); throw error }
    }

    fun exactReplace(target: PreparedFileTarget, expectedSha256: String, replacements: List<ExactReplacement>,
                     owner: PhoneControlEffectOwner, stillAuthorized: () -> Boolean): AndroidProviderResult =
        AndroidFileMutationCoordinator.withExclusivePath(File(target.canonicalPath)) {
            when (val prepared = prepareFileEditLocked(target, expectedSha256) { original, bodyLimit ->
                ExactTextEditPlanner.planOrdinary(File(target.canonicalPath), original, replacements, bodyLimit)
            }) {
                is PreparedFileEditResult.Failure -> prepared.result
                is PreparedFileEditResult.Ready -> commitPreparedEdit(prepared.edit, "content", owner, stillAuthorized)
            }
        }

    fun structuralPreflight(path: String, expectedSha256: String, replacements: List<ExactReplacement>,
                            suppliedToken: String?): AndroidProviderResult {
        val target = try { prepareTarget(path) } catch (error: Throwable) { return preparationFailure(error) }
        return target.use {
            when (val prepared = prepareFileEditLocked(target, expectedSha256) { original, bodyLimit ->
                ExactTextEditPlanner.planStructural(File(target.canonicalPath), original, replacements, suppliedToken, bodyLimit)
            }) {
                is PreparedFileEditResult.Failure -> prepared.result
                is PreparedFileEditResult.Ready -> AndroidProviderResult.Success(buildJsonObject {
                    editEvidence(prepared.edit, "structure").forEach { (key, value) -> put(key, value) }
                    put("ready_for_request_contract_check", true)
                    put("original_unchanged", true)
                })
            }
        }
    }

    fun commitStructuralAfterAuthorization(target: PreparedFileTarget, expectedSha256: String,
        replacements: List<ExactReplacement>, suppliedToken: String,
        owner: PhoneControlEffectOwner, stillAuthorized: () -> Boolean): AndroidProviderResult =
        AndroidFileMutationCoordinator.withExclusivePath(File(target.canonicalPath)) {
            when (val prepared = prepareFileEditLocked(target, expectedSha256) { original, bodyLimit ->
                ExactTextEditPlanner.planStructural(File(target.canonicalPath), original, replacements, suppliedToken, bodyLimit)
            }) {
                is PreparedFileEditResult.Failure -> prepared.result
                is PreparedFileEditResult.Ready -> commitPreparedEdit(prepared.edit, "structure", owner, stillAuthorized)
            }
        }

    private fun prepareFileEditLocked(
        target: PreparedFileTarget,
        expectedSha256: String,
        planner: (String, Long) -> TextEditPlan,
    ): PreparedFileEditResult {
        val file = File(target.canonicalPath)
        val original = target.originalBytes()
            ?: return preparedFailure(failure("path_unavailable", "The file is unavailable"))
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
        val bodyLimit = MAX_TEXT_BYTES - if (hasBom) UTF8_BOM_BYTES.size else 0
        val edit = when (val plan = planner(text, bodyLimit)) {
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
                target = target,
                original = original,
                beforeSha256 = beforeSha256,
                hasBom = hasBom,
                edit = edit,
                updated = updated,
            ),
        )
    }

    private fun commitPreparedEdit(prepared: PreparedFileEdit, editScope: String,
                                   owner: PhoneControlEffectOwner, stillAuthorized: () -> Boolean): AndroidProviderResult =
        when (val result = FilePublication.commit(prepared.target, prepared.updated, prepared.updated.sha256(), owner, stillAuthorized)) {
            is AndroidProviderResult.Failure -> {
                val actual = (result.data["actual_sha256"] as? JsonPrimitive)?.contentOrNull
                if (result.code == "ERR_FILE_TARGET_LEASE_CHANGED" && actual != null && !actual.equals(prepared.beforeSha256, true)) {
                    result.copy(code = EXACT_FILE_CHANGED_CODE)
                } else result
            }
            is AndroidProviderResult.Success -> result.copy(data = JsonObject(result.data + editEvidence(prepared, editScope)))
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

    fun preparationFailure(error: Throwable): AndroidProviderResult.Failure = AndroidProviderResult.Failure(
        code = when (error) {
            is TargetPreparationException -> error.code
            is FileSnapshotLimitExceeded -> "ERR_TEXT_FILE_TOO_LARGE"
            else -> "ERR_FILE_TARGET_LEASE_UNAVAILABLE"
        },
        message = error.message ?: "The target could not be prepared",
        retryable = true,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
    )

    private fun resolve(path: String): File? = runCatching {
        File(path.trim()).canonicalFile
    }.getOrNull()

    private fun failure(code: String, message: String) = AndroidProviderResult.Failure(code, message)
}

private data class PreparedFileEdit(
    val target: PreparedFileTarget,
    val original: ByteArray,
    val beforeSha256: String,
    val hasBom: Boolean,
    val edit: PreparedTextEdit,
    val updated: ByteArray,
) {
    val file: File get() = File(target.canonicalPath)
}

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

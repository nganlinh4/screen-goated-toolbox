package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

@Serializable
internal data class CreationPendingCleanup(
    val artifactPath: String,
    val quarantinePath: String,
    val expectedIdentity: String? = null,
    val expectedSize: Long? = null,
    val expectedSha256: String? = null,
    val resolution: String? = null,
    val replacementPath: String? = null,
    val replacementIdentity: String? = null,
    val retainedHistoryEntry: CreationHistoryEntry? = null,
)

internal data class CreationCleanupCandidate(
    val path: String,
    val expectedSize: Long? = null,
    val expectedSha256: String? = null,
    val expectedIdentity: String? = null,
    val snapshotTrustedManagedBytes: Boolean = false,
    val retainedHistoryEntry: CreationHistoryEntry? = null,
) {
    companion object {
        fun trustedManaged(path: String) = CreationCleanupCandidate(
            path = path,
            snapshotTrustedManagedBytes = true,
        )
    }
}

internal class CreationPendingCleanupStore(
    context: Context,
    private val files: CreationFileStore,
) {
    private val target = File(context.filesDir, "creation/pending-cleanup.json")
    private val json = Json { ignoreUnknownKeys = true }
    private val lock = Any()

    fun isolateAndEnqueue(candidates: Collection<CreationCleanupCandidate>): Set<String> =
        synchronized(lock) {
            if (candidates.isEmpty()) return emptySet()
            val records = requireNotNull(read()) {
                "Creation cleanup state is unreadable"
            }.toMutableList()
            val planned = candidates.distinctBy(CreationCleanupCandidate::path)
                .mapNotNull { candidate ->
                    val identity = files.managedPathIdentity(candidate.path) ?: return@mapNotNull null
                    if (!files.exists(identity)) return@mapNotNull null
                    val observedIdentity = files.artifactIdentity(identity)
                    val proof = candidate.cleanupProof(identity, observedIdentity)
                        ?: return@mapNotNull null
                    if (!creationCleanupIdentityMatches(proof.identity, observedIdentity)) {
                        return@mapNotNull null
                    }
                    val isolation = files.planManagedIsolation(identity) ?: return@mapNotNull null
                    CreationPendingCleanup(
                        artifactPath = identity,
                        quarantinePath = isolation.isolated.absolutePath,
                        expectedIdentity = proof.identity,
                        expectedSize = proof.size,
                        expectedSha256 = proof.sha256,
                        retainedHistoryEntry = candidate.retainedHistoryEntry,
                    )
                }
            if (planned.isEmpty()) return emptySet()
            records.removeAll { existing ->
                planned.any { it.artifactPath == existing.artifactPath }
            }
            records += planned
            write(records)

            val captured = mutableSetOf<String>()
            planned.forEach { record ->
                if (advance(records, record.artifactPath)) captured += record.artifactPath
            }
            captured
        }

    fun drain(): List<CreationHistoryEntry> = synchronized(lock) {
            val records = read()?.toMutableList() ?: return@synchronized emptyList()
            repeat(minOf(MAXIMUM_ATTEMPTS_PER_DRAIN, records.size)) {
                val record = records.removeAt(0)
                records += record
                advance(records, record.artifactPath)
            }
            write(records)
            records.mapNotNull(::reattachedCreationHistoryEntry)
        }

    fun acknowledgeReattached(entryIds: Set<String>) {
        if (entryIds.isEmpty()) return
        synchronized(lock) {
            val records = requireNotNull(read()) { "Creation cleanup state is unreadable" }
                .filterNot {
                    it.resolution == CREATION_REATTACH_RESOLUTION &&
                        it.retainedHistoryEntry?.id in entryIds
                }
            write(records)
        }
    }

    private fun advance(
        records: MutableList<CreationPendingCleanup>,
        artifactPath: String,
    ): Boolean {
        var record = records.firstOrNull { it.artifactPath == artifactPath } ?: return true
        if (record.resolution == CREATION_REATTACH_RESOLUTION) return false
        val isolation = record.isolation()
        if (record.resolution != null) {
            return resolve(records, record, isolation)
        }

        val originalExists = files.exists(record.artifactPath)
        val quarantineExists = files.exists(record.quarantinePath)
        if (!quarantineExists && originalExists) {
            val originalIdentity = files.artifactIdentity(record.artifactPath)
            if (!creationCleanupIdentityMatches(record.expectedIdentity, originalIdentity)) {
                removeAndWrite(records, record.artifactPath)
                return true
            }
            if (!files.isolateManagedPathIfIdentity(isolation, record.expectedIdentity)) {
                return false
            }
        } else if (!quarantineExists) {
            removeAndWrite(records, record.artifactPath)
            return true
        }

        val quarantineIdentity = files.artifactIdentity(record.quarantinePath)
        if (!creationCleanupIdentityMatches(record.expectedIdentity, quarantineIdentity)) {
            return preserveUnownedQuarantine(records, record, isolation)
        }
        val actualSize = files.size(record.quarantinePath)
        val actualDigest = runCatching { files.sha256(record.quarantinePath) }.getOrNull()
        val observedDecision = decideCreationCleanup(
            record.expectedSize,
            record.expectedSha256,
            actualSize,
            actualDigest,
            files.exists(record.artifactPath),
            record.expectedIdentity,
            quarantineIdentity,
        )
        val decision = if (
            record.retainedHistoryEntry != null &&
            observedDecision == CreationCleanupDecision.RESTORE
        ) {
            CreationCleanupDecision.RELINQUISH
        } else {
            observedDecision
        }
        if (decision == CreationCleanupDecision.RETRY) return true
        if (decision != CreationCleanupDecision.DELETE) {
            return resolve(
                records,
                prepareResolution(records, record, isolation, requireNotNull(decision.wireName)),
                isolation,
            )
        }
        if (!files.deleteManagedPathIfIdentity(
                record.quarantinePath,
                requireNotNull(record.expectedIdentity),
            )
        ) {
            val stillOwned = creationCleanupIdentityMatches(
                record.expectedIdentity,
                files.artifactIdentity(record.quarantinePath),
            )
            return if (stillOwned) true else preserveUnownedQuarantine(records, record, isolation)
        }
        removeAndWrite(records, record.artifactPath)
        return true
    }

    private fun preserveUnownedQuarantine(
        records: MutableList<CreationPendingCleanup>,
        record: CreationPendingCleanup,
        isolation: CreationFileIsolation,
    ): Boolean {
        val preserved = record.copy(
            resolution = if (files.exists(record.artifactPath)) "relinquish" else "restore",
            replacementPath = null,
            replacementIdentity = null,
            retainedHistoryEntry = null,
        )
        return resolve(
            records,
            prepareResolution(
                records,
                preserved,
                isolation,
                requireNotNull(preserved.resolution),
            ),
            isolation,
        )
    }

    private fun prepareResolution(
        records: MutableList<CreationPendingCleanup>,
        record: CreationPendingCleanup,
        isolation: CreationFileIsolation,
        requestedResolution: String,
    ): CreationPendingCleanup {
        val resolution = if (
            requestedResolution == "restore" && files.exists(record.artifactPath)
        ) "relinquish" else requestedResolution
        val replacementPath = if (resolution == "restore") {
            record.artifactPath
        } else {
            requireNotNull(files.planRelinquishedManagedPath(isolation)) {
                "Could not reserve retained creation artifact"
            }
        }
        val prepared = record.copy(
            resolution = resolution,
            replacementPath = replacementPath,
            replacementIdentity = requireNotNull(files.artifactIdentity(record.quarantinePath)) {
                "Retained creation artifact identity is unavailable"
            },
        )
        replaceAndWrite(records, prepared)
        return prepared
    }

    private fun resolve(
        records: MutableList<CreationPendingCleanup>,
        record: CreationPendingCleanup,
        isolation: CreationFileIsolation,
    ): Boolean {
        if (record.replacementPath == null || record.replacementIdentity == null) {
            return resolve(
                records,
                prepareResolution(
                    records,
                    record,
                    isolation,
                    requireNotNull(record.resolution),
                ),
                isolation,
            )
        }
        val replacementPath = record.replacementPath
        val quarantineExists = files.exists(record.quarantinePath)
        if (!quarantineExists) {
            return finishResolution(records, record)
        }
        if (files.exists(replacementPath)) {
            val replanned = prepareResolution(records, record, isolation, "relinquish")
            return resolve(records, replanned, isolation)
        }
        if (!files.resolveManagedIsolation(isolation, replacementPath)) return false
        return finishResolution(records, record)
    }

    private fun finishResolution(
        records: MutableList<CreationPendingCleanup>,
        record: CreationPendingCleanup,
    ): Boolean {
        val replacementPath = requireNotNull(record.replacementPath)
        if (!creationCleanupResolutionCanFinish(
                record,
                files.artifactIdentity(replacementPath),
            )) return false
        if (record.retainedHistoryEntry != null) {
            replaceAndWrite(
                records,
                record.copy(
                    resolution = CREATION_REATTACH_RESOLUTION,
                ),
            )
            return false
        }
        removeAndWrite(records, record.artifactPath)
        return record.resolution == "relinquish"
    }

    private fun CreationCleanupCandidate.cleanupProof(
        path: String,
        observedIdentity: String?,
    ): CreationCleanupProof? {
        val supplied = expectedIdentity?.takeIf(::isCreationArtifactIdentity)?.let { identity ->
            expectedSize?.takeIf { it >= 0L }?.let { size ->
                expectedSha256?.takeIf(::isCreationSha256)?.let {
                    CreationCleanupProof(identity, size, it.lowercase())
                }
            }
        }
        if (supplied != null || !snapshotTrustedManagedBytes) return supplied
        val identity = observedIdentity?.takeIf(::isCreationArtifactIdentity) ?: return null
        val size = files.size(path).takeIf { it >= 0L } ?: return null
        val digest = runCatching { files.sha256(path) }.getOrNull()
            ?.takeIf(::isCreationSha256)
            ?: return null
        return CreationCleanupProof(identity, size, digest.lowercase())
    }

    private fun replaceAndWrite(
        records: MutableList<CreationPendingCleanup>,
        updated: CreationPendingCleanup,
    ) {
        val index = records.indexOfFirst { it.artifactPath == updated.artifactPath }
        if (index >= 0) records[index] = updated else records += updated
        write(records)
    }

    private fun removeAndWrite(
        records: MutableList<CreationPendingCleanup>,
        artifactPath: String,
    ) {
        records.removeAll { it.artifactPath == artifactPath }
        write(records)
    }

    private fun CreationPendingCleanup.isolation() = CreationFileIsolation(
        original = File(artifactPath),
        isolated = File(quarantinePath),
    )

    private fun read(): List<CreationPendingCleanup>? {
        if (!target.isFile) return emptyList()
        val text = readCreationIndexTextBounded(target, CREATION_CLEANUP_INDEX_MAX_BYTES)
            ?: return null
        return runCatching { json.decodeFromString<List<CreationPendingCleanup>>(text) }
            .getOrNull()
    }

    private fun write(records: List<CreationPendingCleanup>) {
        writeCreationIndexTextAtomically(
            target,
            json.encodeToString(records),
            CREATION_CLEANUP_INDEX_MAX_BYTES,
        )
    }

    private companion object {
        const val MAXIMUM_ATTEMPTS_PER_DRAIN = 32
    }
}

internal const val CREATION_CLEANUP_INDEX_MAX_BYTES = 4L * 1024 * 1024
internal const val CREATION_REATTACH_RESOLUTION = "reattach"

internal fun reattachedCreationHistoryEntry(
    record: CreationPendingCleanup,
): CreationHistoryEntry? {
    if (record.resolution != CREATION_REATTACH_RESOLUTION) return null
    val path = record.replacementPath ?: return null
    return record.retainedHistoryEntry?.copy(
        outputPath = path,
        outputName = if (isUserOwnedCreationOutputPath(path)) {
            record.retainedHistoryEntry.outputName
        } else {
            File(path).name
        },
        committedSize = null,
        committedSha256 = null,
        committedIdentity = null,
    )
}

private fun isCreationSha256(value: String): Boolean =
    value.length == 64 && value.all { it in '0'..'9' || it.lowercaseChar() in 'a'..'f' }

private fun isCreationArtifactIdentity(value: String): Boolean =
    value.isNotBlank() && value.length <= 1_024 && value.none(Char::isISOControl)

private data class CreationCleanupProof(
    val identity: String,
    val size: Long,
    val sha256: String,
)

internal fun creationCleanupIdentityMatches(expected: String?, actual: String?): Boolean =
    expected?.takeIf(::isCreationArtifactIdentity) == actual?.takeIf(::isCreationArtifactIdentity) &&
        expected != null

internal fun creationCleanupResolutionCanFinish(
    record: CreationPendingCleanup,
    actualReplacementIdentity: String?,
): Boolean = record.replacementPath != null &&
    creationCleanupIdentityMatches(record.replacementIdentity, actualReplacementIdentity)

internal enum class CreationCleanupDecision(val wireName: String?) {
    DELETE(null),
    RESTORE("restore"),
    RELINQUISH("relinquish"),
    RETRY(null),
}

internal fun decideCreationCleanup(
    expectedSize: Long?,
    expectedSha256: String?,
    actualSize: Long,
    actualSha256: String?,
    originalExists: Boolean,
    expectedIdentity: String?,
    actualIdentity: String?,
): CreationCleanupDecision {
    if (actualSize < 0L || actualSha256 == null) return CreationCleanupDecision.RETRY
    val proofMatches = creationCleanupIdentityMatches(expectedIdentity, actualIdentity) &&
        expectedSize != null &&
        expectedSha256 != null &&
        expectedSize == actualSize &&
        expectedSha256.equals(actualSha256, ignoreCase = true)
    if (proofMatches) return CreationCleanupDecision.DELETE
    return if (originalExists) {
        CreationCleanupDecision.RELINQUISH
    } else {
        CreationCleanupDecision.RESTORE
    }
}

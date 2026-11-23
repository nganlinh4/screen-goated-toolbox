package dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio

import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidFileMutationCoordinator
import dev.screengoated.toolbox.mobile.phonecontrol.provider.sha256
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import java.util.UUID
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Complete staging, one atomic namespace operation, then an audit of both versions. */
internal object FilePublication {
    fun commit(target: PreparedFileTarget, bytes: ByteArray, expectedSha256: String,
               owner: PhoneControlEffectOwner, stillAuthorized: () -> Boolean,
               admitSource: () -> Boolean = { true }): AndroidProviderResult =
        AndroidFileMutationCoordinator.withExclusivePath(java.io.File(target.canonicalPath)) {
            commitLocked(target, bytes, expectedSha256, owner, stillAuthorized, admitSource)
        }

    private fun commitLocked(target: PreparedFileTarget, bytes: ByteArray, expectedSha256: String,
                             owner: PhoneControlEffectOwner, stillAuthorized: () -> Boolean,
                             admitSource: () -> Boolean): AndroidProviderResult {
        if (!target.consume()) return rejected("ERR_FILE_TARGET_LEASE_CHANGED", "Prepared target is no longer available")
        if (!admitSource()) return rejected("artifact_store_closed", "The source session has retired")
        val effect = owner.beginEffect() ?: return rejected("ERR_CANCELLED", "The job was cancelled")
        var stageName: String? = null
        var staged: TransactionFile? = null
        var namespaceAttempted = false
        var published = false
        var stageAttempted = false
        var targetMayHaveChanged = false
        var backupCandidate = false
        var retainedSnapshot: Snapshot? = null
        var result: AndroidProviderResult
        try {
            try {
                if (owner.isCancellationRequested) throw FileWriteCancelled()
                if (!stillAuthorized()) throw FileAuthorizationExpired()
                require(bytes.size <= target.maximumBytes && bytes.sha256() == expectedSha256) { "Prepared source changed or exceeded its limit" }
                target.revalidate()
                target.materializeParents(effect, stillAuthorized)
                repeat(32) {
                    if (staged != null) return@repeat
                    if (owner.isCancellationRequested) throw FileWriteCancelled()
                    val name = ".sgt-file-${UUID.randomUUID()}.tmp"
                    stageName = name
                    try {
                        val admitted = effect.dispatchBooleanIfActive {
                            stageAttempted = true
                            staged = target.open(name, create = true) ?: error("Created stage is unavailable")
                            true
                        }
                        if (admitted == null) throw FileWriteCancelled()
                    } catch (_: FileEntryAlreadyExists) {
                        stageName = null
                        stageAttempted = false
                    }
                }
                val stage = staged ?: error("No unique staging entry is available")
                stage.writeSynced(bytes) { owner.isCancellationRequested }
                check(stage.readBounded(target.maximumBytes).sha256() == expectedSha256) { "Staged bytes failed verification" }
                if (owner.isCancellationRequested) throw FileWriteCancelled()
                if (!stillAuthorized()) throw FileAuthorizationExpired()
                target.revalidate()
                val namedStage = snapshot(target, checkNotNull(stageName))
                check(namedStage?.id == stage.identity && namedStage.sha256 == expectedSha256) {
                    "The staging entry changed before publication"
                }
                val admitted = effect.dispatchBooleanIfActive {
                    if (!stillAuthorized()) false else {
                        namespaceAttempted = true
                        target.publish(checkNotNull(stageName), exchange = target.lease.existedBefore)
                        published = true
                        true
                    }
                }
                if (admitted == null) throw FileWriteCancelled()
                if (!namespaceAttempted) throw FileAuthorizationExpired()
                targetMayHaveChanged = true
                val current = snapshot(target, target.name)
                val recovery = if (target.lease.existedBefore) snapshot(target, checkNotNull(stageName)) else null
                backupCandidate = target.lease.existedBefore
                retainedSnapshot = recovery
                check(current?.id == stage.identity && current.sha256 == expectedSha256 && current.size == bytes.size) {
                    "Installed file identity or bytes could not be verified"
                }
                if (target.lease.existedBefore) check(recovery != null && recovery.id == target.lease.objectIdentity &&
                    recovery.sha256 == target.lease.expectedSha256) { "The displaced version changed; recovery is retained" }
                target.revalidateRoute()
                result = AndroidProviderResult.Success(JsonObject(data(target, stageName, stage.identity, published, backupCandidate, retainedSnapshot) + buildJsonObject {
                    put("sha256", expectedSha256); put("byte_count", bytes.size); put("bytes", bytes.size)
                    put("atomic", true); put("original_unchanged", false)
                }), effectMayHaveOccurred = true, effectVerified = true)
            } catch (error: Throwable) {
                if (namespaceAttempted && !published) {
                    val current = snapshot(target, target.name)
                    retainedSnapshot = stageName?.let { snapshot(target, it) }
                    val stageStillOwned = staged?.let { retainedSnapshot?.id == it.identity } == true
                    // A failed observation cannot prove absence or that an ambiguous syscall had no effect.
                    targetMayHaveChanged = (error !is FileEntryAlreadyExists && error !is AtomicPublicationUnavailable) ||
                        (staged != null && current?.id == staged.identity)
                    backupCandidate = target.lease.existedBefore && !stageStillOwned
                }
                if (!namespaceAttempted || (published && target.lease.existedBefore && retainedSnapshot == null)) {
                    retainedSnapshot = stageName?.let { snapshot(target, it) }
                }
                val metadata = data(target, stageName, staged?.identity, published, backupCandidate, retainedSnapshot)
                val effectOccurred = stageAttempted || target.attemptedParents.isNotEmpty() || targetMayHaveChanged || published
                val code = when {
                    published || targetMayHaveChanged -> "ERR_FILE_TRANSACTION_UNVERIFIED"
                    error is FileWriteCancelled -> "ERR_CANCELLED"
                    error is FileAuthorizationExpired -> "ERR_FILE_TARGET_REQUEST_CONTRACT_UNVERIFIED"
                    error is AtomicPublicationUnavailable -> "ERR_FILE_ATOMIC_PUBLICATION_UNAVAILABLE"
                    error is FileEntryAlreadyExists -> "path_exists"
                    error is FileTargetVersionChanged -> "ERR_FILE_TARGET_LEASE_CHANGED"
                    !stageAttempted && target.attemptedParents.isEmpty() -> "ERR_FILE_TARGET_LEASE_CHANGED"
                    else -> "ERR_FILE_TRANSACTION_FAILED"
                }
                result = AndroidProviderResult.Failure(code, error.message ?: "The file transaction failed",
                    retryable = !published && !targetMayHaveChanged && error !is AtomicPublicationUnavailable,
                    data = JsonObject(metadata + buildJsonObject {
                        put("target_effect_may_have_occurred", targetMayHaveChanged || published)
                        put("original_unchanged", !targetMayHaveChanged && !published)
                        put("effect_verified", false)
                        if (error is FileTargetVersionChanged) error.actualSha256?.let { put("actual_sha256", it) }
                        if (error is FileTargetVersionChanged && error.appeared) put("target_appeared", true)
                    }), effect = if (effectOccurred) EffectCertainty.MAY_HAVE_OCCURRED else EffectCertainty.PROVEN_NO_EFFECT,
                    providerState = if (error is AtomicPublicationUnavailable) dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState.UNSUPPORTED else null)
            }
            // Preserve the critical paths even if cancellation wins the ordinary response race.
            val metadata = when (result) {
                is AndroidProviderResult.Success -> result.data
                is AndroidProviderResult.Failure -> result.data
            }
            effect.recordTerminalEvidence(buildJsonObject {
                put("kind", "file_transaction")
                for (field in listOf("path", "retained_staging_path", "retained_backup", "retained_identity", "recovery_sha256", "created_parent_count", "created_parent_paths", "attempted_parent_paths")) {
                    metadata[field]?.let { put(field, it) }
                }
            })
            return result
        } finally {
            runCatching { staged?.close() }
            effect.close()
        }
    }

    private data class Snapshot(val id: FileObjectId, val sha256: String, val size: Int)
    private fun snapshot(target: PreparedFileTarget, name: String): Snapshot? = runCatching {
        target.open(name).use { file -> file?.readBounded(target.maximumBytes)?.let { Snapshot(file.identity, it.sha256(), it.size) } }
    }.getOrNull()

    private fun data(target: PreparedFileTarget, stage: String?, stagedId: FileObjectId?, published: Boolean, backup: Boolean, retained: Snapshot?): JsonObject = buildJsonObject {
        put("path", target.currentPath() ?: target.canonicalPath)
        put("prepared_canonical_path", target.canonicalPath)
        put("created_parent_count", target.createdParents.size)
        put("created_parent_paths", JsonArray(target.createdParents.map(::JsonPrimitive)))
        put("attempted_parent_paths", JsonArray(target.attemptedParents.map(::JsonPrimitive)))
        if (stage != null && (backup || !published)) {
            val observed = target.currentPath(stage)
            put(if (backup) "retained_backup" else "retained_staging_path", observed ?: java.io.File(java.io.File(target.canonicalPath).parent, stage).path)
            put("retained_location_observed", observed != null && retained != null)
            retained?.let { put("retained_identity", it.id.description()); put("recovery_sha256", it.sha256) }
            stagedId?.let { put("expected_stage_identity", it.description()) }
            put("retained_entry_ownership_unverified", if (backup) retained == null || retained.id != target.lease.objectIdentity ||
                retained.sha256 != target.lease.expectedSha256 else retained?.id != stagedId)
        }
    }

    private fun rejected(code: String, message: String) = AndroidProviderResult.Failure(code, message,
        retryable = true, effect = EffectCertainty.PROVEN_NO_EFFECT)
}

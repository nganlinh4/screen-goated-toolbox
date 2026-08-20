package dev.screengoated.toolbox.mobile.creation

import java.io.File

internal const val CREATION_DELIVERY_INDEX_MAX_BYTES = 4L * 1024 * 1024
internal const val CREATION_DELIVERY_MAXIMUM_RECORDS = 384
internal const val CREATION_DELIVERY_MAXIMUM_CLEANUP_ATTEMPTS = 1_024

internal fun validCreationDeliveryRecord(
    filesDir: File,
    record: CreationDeliveryRecord,
): Boolean {
    val tool = CreationTool.fromWireName(record.request.tool) ?: return false
    val maximumBytes = maximumCreationResultBytes(tool)
    val expectedMime = when (tool) {
        CreationTool.IMAGE_TO_3D -> "model/gltf-binary"
        CreationTool.IMAGE_TO_SVG -> "image/svg+xml"
        CreationTool.IMAGE_CREATOR -> "image/png"
    }
    val token = record.intent.reservationToken
    val library = File(filesDir, "creation/library").toPath().toAbsolutePath().normalize()
    val intentValid = token != null &&
        token.length == 32 &&
        token.all(Char::isCreationHexDigit) &&
        record.intent.mimeType == expectedMime &&
        record.intent.finalName == safeCreationOutputName(record.intent.finalName) &&
        when (record.intent.kind) {
            "managed" -> {
                val target = record.intent.targetPath?.let(::File)?.toPath()
                    ?.toAbsolutePath()?.normalize()
                val pending = record.intent.pendingPath?.let(::File)?.toPath()
                    ?.toAbsolutePath()?.normalize()
                record.intent.destination == null &&
                    record.intent.pendingName == null &&
                    target?.parent == library &&
                    target.fileName.toString() == record.intent.finalName &&
                    pending?.parent == library &&
                    pending.fileName.toString() == ".sgt-$token.delivery"
            }
            "saf" -> record.intent.destination?.startsWith("content://") == true &&
                record.intent.destination.length <= 2_048 &&
                record.intent.targetPath == null &&
                record.intent.pendingPath == null &&
                record.intent.pendingName == ".sgt-$token.pending"
            else -> false
        }
    val pendingValid = if (record.pendingHandle == null) {
        record.pendingIdentity == null
    } else {
        !record.pendingIdentity.isNullOrBlank() &&
            record.pendingIdentity.length <= 2_048 &&
            if (record.intent.kind == "managed") {
                record.pendingHandle == record.intent.pendingPath
            } else {
                record.pendingHandle.startsWith("content://")
            }
    }
    val publishedValid = record.publishedPath == null ||
        if (record.intent.kind == "managed") {
            record.publishedPath == record.intent.targetPath
        } else {
            record.publishedPath.startsWith("content://")
        }
    val stageValid = when (record.transactionStage) {
        "validated" -> !record.publicationPrepared &&
            record.publishedPath == null &&
            !record.historyCommitted
        "publication_prepared" -> record.publicationPrepared &&
            record.pendingHandle != null &&
            (record.publishedPath == null ||
                record.companion?.publishedPath == null) &&
            !record.historyCommitted
        "published" -> record.publicationPrepared &&
            record.pendingHandle != null &&
            record.publishedPath != null &&
            !record.historyCommitted
        "history_committed" -> record.publicationPrepared &&
            record.pendingHandle != null &&
            record.publishedPath != null &&
            record.historyCommitted
        else -> false
    }
    val companionValid = record.companion?.let { companion ->
        val primary = File(record.sealedPath).absoluteFile.normalize()
        val sealed = File(companion.sealedPath).absoluteFile.normalize()
        val companionToken = companion.intent.reservationToken
        val assignmentValid = sealed.parentFile == primary.parentFile &&
            sealed.nameWithoutExtension == primary.nameWithoutExtension &&
            sealed.extension.equals("fbx", ignoreCase = true) &&
            companion.outputName == sealed.name &&
            record.event.downloadPath == companion.sealedPath &&
            record.event.downloadName == companion.outputName
        val companionIntentValid = companionToken != null &&
            companionToken.length == 32 &&
            companionToken.all(Char::isCreationHexDigit) &&
            companion.intent.mimeType == "application/octet-stream" &&
            companion.intent.finalName == companion.outputName &&
            companion.intent.kind == record.intent.kind &&
            companion.intent.destination == record.intent.destination &&
            when (companion.intent.kind) {
                "managed" -> {
                    val target = companion.intent.targetPath?.let(::File)?.toPath()
                        ?.toAbsolutePath()?.normalize()
                    val pending = companion.intent.pendingPath?.let(::File)?.toPath()
                        ?.toAbsolutePath()?.normalize()
                    companion.intent.pendingName == null &&
                        target?.parent == library &&
                        target.fileName.toString() == companion.outputName &&
                        pending?.parent == library &&
                        pending.fileName.toString() == ".sgt-$companionToken.delivery"
                }
                "saf" -> companion.intent.targetPath == null &&
                    companion.intent.pendingPath == null &&
                    companion.intent.pendingName == ".sgt-$companionToken.pending"
                else -> false
            }
        val pendingCompanionValid = if (companion.pendingHandle == null) {
            companion.pendingIdentity == null && !companion.publicationPrepared &&
                companion.publishedPath == null
        } else {
            !companion.pendingIdentity.isNullOrBlank() &&
                companion.pendingIdentity.length <= 2_048 &&
                (record.transactionStage !in setOf("published", "history_committed") ||
                    companion.publicationPrepared) &&
                if (companion.intent.kind == "managed") {
                    companion.pendingHandle == companion.intent.pendingPath
                } else {
                    companion.pendingHandle.startsWith("content://")
                }
        }
        val publishedCompanionValid = companion.publishedPath == null ||
            if (companion.intent.kind == "managed") {
                companion.publishedPath == companion.intent.targetPath
            } else {
                companion.publishedPath.startsWith("content://")
            }
        assignmentValid && companionIntentValid && pendingCompanionValid &&
            publishedCompanionValid &&
            companion.artifactSize in 21..maximumBytes &&
            companion.artifactSha256.length == 64 &&
            companion.artifactSha256.all(Char::isCreationHexDigit) &&
            (record.transactionStage != "published" &&
                record.transactionStage != "history_committed" ||
                companion.publishedPath != null)
    } ?: (record.event.downloadPath == null && record.event.downloadName == null)
    return record.dispatchId.length in 1..256 &&
        record.dispatchId == record.request.dispatchId &&
        creationRequestHasValidDeliveryIdentity(record.request) &&
        record.engineId.length in 1..256 &&
        record.ownerId.length in 1..256 &&
        record.current.jobId == record.request.jobId &&
        (record.event.jobId == null || record.event.jobId == record.request.jobId) &&
        (record.event.outputPath == null || record.event.outputPath == record.sealedPath) &&
        record.sealedPath == record.request.outputPath &&
        isReservedCreationStagingPath(filesDir, tool, record.sealedPath) &&
        record.mimeType == expectedMime &&
        record.artifactSize in 1..maximumBytes &&
        record.artifactSha256.length == 64 &&
        record.artifactSha256.all(Char::isCreationHexDigit) &&
        record.cleanupAttempts in 0..CREATION_DELIVERY_MAXIMUM_CLEANUP_ATTEMPTS &&
        intentValid &&
        pendingValid &&
        publishedValid &&
        stageValid &&
        companionValid
}

internal fun retainCreationDeliveryRecords(
    records: List<CreationDeliveryRecord>,
    sealedExists: (String) -> Boolean,
): List<CreationDeliveryRecord> = records.filterNot {
    it.historyCommitted &&
        !sealedExists(it.sealedPath) &&
        (it.companion == null || !sealedExists(it.companion.sealedPath))
}

private fun Char.isCreationHexDigit(): Boolean =
    this in '0'..'9' || this in 'a'..'f' || this in 'A'..'F'

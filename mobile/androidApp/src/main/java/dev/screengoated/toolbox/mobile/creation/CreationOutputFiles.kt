package dev.screengoated.toolbox.mobile.creation

import android.net.Uri
import java.io.File
import java.io.FileOutputStream
import java.io.InputStream
import java.nio.channels.FileChannel
import java.nio.file.StandardOpenOption
import java.security.MessageDigest
import java.util.UUID

internal const val CREATION_OUTPUT_NAME_MAXIMUM_CHARACTERS = 180

internal fun forceCreationDirectory(directory: File) {
    runCatching {
        FileChannel.open(directory.toPath(), StandardOpenOption.READ).use { it.force(true) }
    }
}

internal fun copyCreationFileDurably(source: File, target: File) {
    check(target.createNewFile()) { "Could not reserve output file" }
    try {
        writeCreationFileDurably(source, target)
    } catch (failure: Throwable) {
        target.delete()
        throw failure
    }
}

internal fun writeCreationFileDurably(source: File, target: File) {
    FileOutputStream(target, false).use { output ->
        source.inputStream().use { it.copyTo(output) }
        output.fd.sync()
    }
}

internal fun creationFileMatchesProof(
    file: File,
    expectedSize: Long,
    expectedSha256: String,
): Boolean = file.isFile &&
    file.length() == expectedSize &&
    runCatching {
        creationFileSha256(file).equals(expectedSha256, ignoreCase = true)
    }.getOrDefault(false)

internal fun creationFileSha256(file: File): String =
    file.inputStream().use(::creationStreamSha256)

internal fun creationStreamSha256(input: InputStream): String {
    val digest = MessageDigest.getInstance("SHA-256")
    val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
    while (true) {
        val read = input.read(buffer)
        if (read < 0) break
        digest.update(buffer, 0, read)
    }
    return digest.digest().joinToString("") { "%02x".format(it) }
}

internal fun creationStreamMatchesProof(
    input: InputStream,
    expectedSize: Long,
    expectedSha256: String,
): Boolean {
    if (expectedSize < 0L || expectedSha256.length != 64) return false
    val digest = MessageDigest.getInstance("SHA-256")
    val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
    var observedSize = 0L
    while (true) {
        val read = input.read(buffer)
        if (read < 0) break
        observedSize += read
        if (observedSize > expectedSize) return false
        digest.update(buffer, 0, read)
    }
    val observedSha256 = digest.digest().joinToString("") { "%02x".format(it) }
    return observedSize == expectedSize && observedSha256.equals(expectedSha256, true)
}

internal fun uniqueCreationDeliveryName(
    requested: String,
    occupied: Set<String>,
    dispatchId: String,
): String = uniqueCreationDeliveryName(requested, { it in occupied }, dispatchId)

internal fun uniqueCreationDeliveryName(
    requested: String,
    occupied: (String) -> Boolean,
    dispatchId: String,
): String {
    if (!occupied(requested)) return requested
    val dot = requested.lastIndexOf('.')
    val stem = if (dot > 0) requested.substring(0, dot) else requested
    val extension = if (dot > 0) requested.substring(dot) else ""
    val suffix = dispatchId.filter(Char::isLetterOrDigit).takeLast(12).ifBlank { "result" }
    val boundedStem = stem.take(
        (CREATION_OUTPUT_NAME_MAXIMUM_CHARACTERS - extension.length - suffix.length - 1)
            .coerceAtLeast(1),
    )
    var candidate = "$boundedStem-$suffix$extension"
    var index = 2
    while (occupied(candidate)) {
        val indexedStem = stem.take(
            (CREATION_OUTPUT_NAME_MAXIMUM_CHARACTERS - extension.length -
                suffix.length - index.toString().length - 2).coerceAtLeast(1),
        )
        candidate = "$indexedStem-$suffix-$index$extension"
        index += 1
    }
    return candidate
}

internal fun uniqueCreationDownloadsName(
    requested: String,
    occupied: (String) -> Boolean,
    dispatchId: String,
): String = uniqueCreationDeliveryName(
    requested,
    { candidate -> candidate == requested || occupied(candidate) },
    dispatchId,
)

internal fun creationHistoryRenameNames(
    requestedName: String,
    companionName: String?,
    primaryOccupied: Set<String>,
    companionOccupied: Set<String>,
    transactionId: String,
): Pair<String, String?> = creationHistoryRenameNames(
    requestedName,
    companionName,
    { it in primaryOccupied },
    { it in companionOccupied },
    transactionId,
)

internal fun creationHistoryRenameNames(
    requestedName: String,
    companionName: String?,
    primaryOccupied: (String) -> Boolean,
    companionOccupied: (String) -> Boolean,
    transactionId: String,
): Pair<String, String?> {
    if (companionName == null) {
        return uniqueCreationDeliveryName(
            requestedName,
            primaryOccupied,
            transactionId,
        ) to null
    }
    val companionExtension = companionName.substringAfterLast('.', "")
    val rejected = mutableSetOf<String>()
    while (true) {
        val primary = uniqueCreationDeliveryName(requestedName, { name ->
            primaryOccupied(name) || name in rejected
        }, transactionId)
        val stem = primary.substringBeforeLast('.', primary)
        val companion = if (companionExtension.isBlank()) stem else "$stem.$companionExtension"
        if (!companionOccupied(companion)) return primary to companion
        rejected += primary
    }
}

internal fun safeCreationOutputName(value: String): String = value
    .substringAfterLast('/')
    .substringAfterLast('\\')
    .map { if (it.isLetterOrDigit() || it in "._-") it else '_' }
    .joinToString("")
    .trim('.', ' ')
    .take(CREATION_OUTPUT_NAME_MAXIMUM_CHARACTERS)
    .ifBlank { "result" }

internal fun creationDownloadsPendingName(token: String, finalName: String): String {
    require(token.length == 32 && token.all { it.isDigit() || it in 'a'..'f' })
    val extension = safeCreationOutputName(finalName)
        .substringAfterLast('.', "")
        .filter(Char::isLetterOrDigit)
        .take(16)
        .lowercase()
    return "sgt-$token.pending" + extension.takeIf(String::isNotEmpty)?.let { ".$it" }.orEmpty()
}

internal fun uniqueCreationOutputFile(directory: File, requested: String): File {
    val first = File(directory, requested)
    if (!first.exists()) return first
    val dot = requested.lastIndexOf('.')
    val stem = if (dot > 0) requested.substring(0, dot) else requested
    val extension = if (dot > 0) requested.substring(dot) else ""
    repeat(9_998) { offset ->
        val candidate = File(directory, "${stem}_${offset + 2}$extension")
        if (!candidate.exists()) return candidate
    }
    return File(directory, "${stem}_${UUID.randomUUID()}$extension")
}

internal data class CreationPendingReservation(
    val handle: String,
    val identity: String,
)

internal fun creationSafRenameRecoveryMatches(
    expectedIdentity: String,
    oldHandleExists: Boolean,
    oldHandleIdentity: String?,
    targetIdentity: String?,
): Boolean = targetIdentity == expectedIdentity &&
    (!oldHandleExists || oldHandleIdentity == expectedIdentity)

internal val managedDeliveryLock = Any()

internal fun String.creationContentUri(): Uri? =
    takeIf { startsWith("content://") }?.let(Uri::parse)

internal fun CreationTool.usesDownloadsByDefault(): Boolean =
    this == CreationTool.IMAGE_TO_SVG

internal fun CreationTool.usesProjectLibrary(): Boolean = this == CreationTool.IMAGE_TO_3D

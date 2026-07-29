package dev.screengoated.toolbox.mobile.creation

import android.net.Uri
import java.io.File
import java.util.UUID

internal fun uniqueCreationManagedFile(directory: File, requested: String): File {
    val safe = safeCreationManagedName(requested)
    val first = File(directory, safe)
    if (!first.exists()) return first
    val dot = safe.lastIndexOf('.')
    val stem = if (dot > 0) safe.substring(0, dot) else safe
    val extension = if (dot > 0) safe.substring(dot) else ""
    repeat(9_998) { offset ->
        val candidate = File(directory, "${stem}_${offset + 2}$extension")
        if (!candidate.exists()) return candidate
    }
    return File(directory, "${stem}_${UUID.randomUUID()}$extension")
}

internal fun safeCreationManagedName(value: String): String = value
    .substringAfterLast('/')
    .substringAfterLast('\\')
    .map { if (it.isLetterOrDigit() || it in "._-") it else '_' }
    .joinToString("")
    .trim('.', ' ')
    .ifBlank { "result" }

internal fun safeCreationManagedStem(value: String): String =
    safeCreationManagedName(value).substringBeforeLast('.').ifBlank { "result" }

internal fun String.creationUriOrNull(): Uri? =
    takeIf { startsWith("content://") }?.let(Uri::parse)

internal fun creationPreviewMaximumBytes(extension: String): Long = when (extension) {
    "glb" -> CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES
    "svg" -> CreationContract.MAXIMUM_SVG_ARTIFACT_BYTES
    "png" -> CreationContract.MAXIMUM_IMAGE_ARTIFACT_BYTES
    else -> error("Unsupported preview type")
}

internal fun validateCreationPreviewArtifact(file: File, extension: String) {
    when (extension) {
        "glb" -> CreationArtifactValidator.validateGlb(file)
        "svg" -> CreationArtifactValidator.validateSvg(file)
        "png" -> CreationArtifactValidator.validatePng(file, null, null)
    }
}

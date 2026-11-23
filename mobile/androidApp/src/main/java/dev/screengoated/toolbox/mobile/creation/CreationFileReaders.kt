package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.io.InputStream

internal fun CreationFileStore.openInput(path: String): InputStream =
    path.creationUriOrNull()?.let(context.contentResolver::openInputStream)
        ?: File(path).inputStream()

internal fun CreationFileStore.readBytes(path: String, maximum: Long): ByteArray {
    val knownSize = size(path)
    require(knownSize < 0 || knownSize <= maximum) { "Preview asset is too large" }
    return openInput(path).use { stream -> readCreationBytesBounded(stream, maximum) }
}

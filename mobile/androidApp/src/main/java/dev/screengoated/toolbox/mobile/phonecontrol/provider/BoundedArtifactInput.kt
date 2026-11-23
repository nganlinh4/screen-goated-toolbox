package dev.screengoated.toolbox.mobile.phonecontrol.provider

import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileInputStream
import java.io.InputStream

internal fun readArtifactFileBounded(file: File, maximumBytes: Int): ByteArray {
    require(maximumBytes >= 0)
    require(file.isFile) { "Artifact source is not a regular file" }
    return FileInputStream(file).use { input ->
        require(input.channel.size() <= maximumBytes) { "Artifact source exceeds the byte limit" }
        readArtifactBytesBounded(input, maximumBytes)
    }
}

internal fun readArtifactBytesBounded(input: InputStream, maximumBytes: Int): ByteArray {
    require(maximumBytes >= 0)
    val output = ByteArrayOutputStream()
    val buffer = ByteArray(8192)
    while (true) {
        val remaining = maximumBytes.toLong() + 1 - output.size()
        val count = input.read(buffer, 0, minOf(buffer.size.toLong(), remaining).toInt())
        if (count < 0) return output.toByteArray()
        check(count > 0) { "Artifact input made no progress" }
        require(output.size().toLong() + count <= maximumBytes) { "Artifact source exceeds the byte limit" }
        output.write(buffer, 0, count)
    }
}

package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlin.coroutines.coroutineContext

internal class CreationPreviewFiles(private val files: CreationFileStore) {
    suspend fun materialize(path: String, extension: String): File = withContext(Dispatchers.IO) {
        if (extension.equals("glb", ignoreCase = true)) {
            creationModelPreviewLane.withLock {
                coroutineContext.ensureActive()
                files.materializePreview(path, extension).also {
                    coroutineContext.ensureActive()
                }
            }
        } else {
            files.materializePreview(path, extension)
        }
    }

    suspend fun wireframe(path: String): File = withContext(Dispatchers.IO) {
        creationModelPreviewLane.withLock {
            coroutineContext.ensureActive()
            val source = files.materializePreview(path, "glb")
            val target = File(source.parentFile, "${source.nameWithoutExtension}.wireframe.glb")
            if (!target.isFile ||
                target.length() == 0L ||
                target.lastModified() < source.lastModified()
            ) {
                CreationWireframeGlb.create(source, target)
            }
            coroutineContext.ensureActive()
            target
        }
    }

    suspend fun readSvg(path: String): String = withContext(Dispatchers.IO) {
        files.readBytes(path, CreationContract.MAXIMUM_SVG_ARTIFACT_BYTES).decodeToString()
    }

    suspend fun saveSvg(path: String, svg: String) = withContext(Dispatchers.IO) {
        CreationArtifactValidator.validateSvgText(svg)
        files.writeText(path, svg)
    }
}

private val creationModelPreviewLane = Mutex()

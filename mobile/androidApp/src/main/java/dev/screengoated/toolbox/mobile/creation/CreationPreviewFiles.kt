package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlin.coroutines.coroutineContext

internal class CreationPreviewFiles(private val files: CreationFileStore) {
    private val viewerModels = CreationViewerModelFiles(files)

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

    suspend fun viewerModel(path: String): File = withContext(Dispatchers.IO) {
        creationModelPreviewLane.withLock {
            coroutineContext.ensureActive()
            viewerModels.materialize(path).also { coroutineContext.ensureActive() }
        }
    }

    fun releaseViewerModel(file: File): Boolean = viewerModels.release(file)

    suspend fun readSvg(path: String): String = withContext(Dispatchers.IO) {
        files.readBytes(path, CreationContract.MAXIMUM_SVG_ARTIFACT_BYTES).decodeToString()
    }

    suspend fun saveSvg(path: String, svg: String) = withContext(Dispatchers.IO) {
        CreationArtifactValidator.validateSvgText(svg)
        files.writeText(path, svg)
    }
}

private val creationModelPreviewLane = Mutex()

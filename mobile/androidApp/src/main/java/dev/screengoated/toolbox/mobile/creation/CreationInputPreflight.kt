package dev.screengoated.toolbox.mobile.creation

import android.graphics.BitmapFactory
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

internal data class CreationInputValidation(val itemId: String, val error: String)

internal fun creationInputError(tool: CreationTool, mode: String, width: Int, height: Int, bytes: Long): String? = when {
    tool == CreationTool.IMAGE_TO_SVG && bytes > 4_490_000 -> "image_too_large"
    width <= 0 || height <= 0 || bytes <= 0 -> "image_invalid"
    tool == CreationTool.IMAGE_TO_3D && mode == "fast" && (width < 32 || height < 32) -> "image_too_small"
    tool == CreationTool.IMAGE_TO_3D && mode == "fast" && bytes > 20_000_000 -> "image_too_large"
    else -> null
}

internal class CreationInputPreflight(
    private val files: CreationFileStore,
    private val tool: CreationTool,
    private val scope: CoroutineScope,
    private val state: MutableStateFlow<CreationNativeUiState>,
    private val closed: () -> Boolean,
    private val accepted: () -> Unit,
) {
    fun submit() {
        val snapshot = state.value.selectedItem ?: return
        if (!snapshot.isConfigurable() || state.value.validatingItemId != null) return
        state.update { it.copy(validatingItemId = snapshot.id, inputValidation = null) }
        scope.launch(Dispatchers.IO) {
            val failure = try {
                inspectCreationInput(files, tool, snapshot.sourcePath, snapshot.generationMode)
            } catch (cancelled: CancellationException) {
                state.update { if (it.validatingItemId == snapshot.id) it.copy(validatingItemId = null) else it }
                throw cancelled
            } catch (_: Exception) { "validation_unavailable" }
            var submitted = false
            state.update { current ->
                if (closed() || current.selectedItem != snapshot || current.validatingItemId != snapshot.id) {
                    current.copy(validatingItemId = null)
                } else if (failure != null) {
                    current.copy(validatingItemId = null, inputValidation = CreationInputValidation(snapshot.id, failure))
                } else {
                    submitted = true
                    current.submitSelectedItem().copy(validatingItemId = null)
                }
            }
            if (submitted && !closed()) accepted()
        }
    }

}

internal fun inspectCreationInput(files: CreationFileStore, tool: CreationTool, sourcePath: String, generationMode: String): String? {
        if (tool == CreationTool.IMAGE_CREATOR) return null
        val bytes = files.size(sourcePath)
        if (bytes < 0) return "validation_unavailable"
        if (tool == CreationTool.IMAGE_TO_SVG && bytes > 4_490_000) return "image_too_large"
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        files.openInput(sourcePath).use { BitmapFactory.decodeStream(it, null, bounds) }
        creationInputError(tool, generationMode, bounds.outWidth, bounds.outHeight, bytes)?.let { return it }
        if (bounds.outWidth > CreationContract.MAXIMUM_IMAGE_DIMENSION ||
            bounds.outHeight > CreationContract.MAXIMUM_IMAGE_DIMENSION ||
            bounds.outWidth.toLong() * bounds.outHeight > CreationContract.MAXIMUM_DECODED_IMAGE_PIXELS ||
            bounds.outMimeType !in setOf("image/png", "image/jpeg", "image/webp")) return "image_invalid"
        var sample = 1
        while (bounds.outWidth / sample > 2048 || bounds.outHeight / sample > 2048) sample *= 2
        val decoded = files.openInput(sourcePath).use {
            BitmapFactory.decodeStream(it, null, BitmapFactory.Options().apply { inSampleSize = sample })
        } ?: return "image_invalid"
        decoded.recycle()
        return null
}

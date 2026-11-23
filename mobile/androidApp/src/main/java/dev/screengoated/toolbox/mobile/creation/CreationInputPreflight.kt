package dev.screengoated.toolbox.mobile.creation

import android.graphics.BitmapFactory
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

internal data class CreationInputValidation(val snapshot: CreationNativeItem, val error: String) {
    val itemId: String get() = snapshot.id
}

internal fun creationInputError(tool: CreationTool, mode: String, width: Int, height: Int, bytes: Long): String? = when {
    tool == CreationTool.IMAGE_TO_SVG && bytes > 4_490_000 -> "image_too_large"
    // Windows validates sources only for Quality and refuses retired modes.
    tool == CreationTool.IMAGE_TO_3D && mode != CreationGenerationMode.QUALITY.wireName -> "validation_unavailable"
    width <= 0 || height <= 0 || bytes <= 0 -> "image_invalid"
    else -> null
}

internal class CreationInputPreflight(
    private val scope: CoroutineScope,
    private val state: MutableStateFlow<CreationNativeUiState>,
    private val closed: () -> Boolean,
    private val accepted: () -> Unit,
    private val inspect: suspend (CreationNativeItem) -> String?,
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    constructor(files: CreationFileStore, tool: CreationTool, scope: CoroutineScope,
        state: MutableStateFlow<CreationNativeUiState>, closed: () -> Boolean, accepted: () -> Unit) :
        this(scope,state,closed,accepted,{ snapshot -> inspectCreationInput(files,tool,snapshot.sourcePath,snapshot.generationMode) })

    private class Attempt(val snapshot: CreationNativeItem) { var job: Job? = null }
    private val gate = Any()
    private var active: Attempt? = null
    private var stopped = false

    fun updateInput(transform: (CreationNativeUiState) -> CreationNativeUiState) = synchronized(gate) {
        if (stopped || closed()) return@synchronized
        while (true) {
            val current = state.value
            val changed = transform(current)
            val targetChanged = current.selectedItem != changed.selectedItem || current.tab != changed.tab ||
                current.selectedHistoryId != changed.selectedHistoryId || current.outputDirectory != changed.outputDirectory
            val next = if (targetChanged) changed.copy(validatingItemId = null, inputValidation = null) else changed
            if (state.compareAndSet(current, next)) {
                if (targetChanged) retireActive()
                return@synchronized
            }
        }
    }

    fun dismiss(validation: CreationInputValidation): Boolean = synchronized(gate) {
        var current = state.value
        while (current.inputValidation === validation) {
            if (state.compareAndSet(current, current.copy(inputValidation = null))) return@synchronized true
            current = state.value
        }
        false
    }

    fun retry(validation: CreationInputValidation) = synchronized(gate) {
        if (dismiss(validation) && !stopped && !closed()) start(validation.snapshot)
    }

    fun cancelPending(itemId: String): Boolean = synchronized(gate) {
        if (active?.snapshot?.id != itemId) return@synchronized false
        retireActive()
        state.update { it.copy(validatingItemId = null) }
        true
    }

    fun invalidate() = synchronized(gate) {
        retireActive()
        state.update { it.copy(validatingItemId = null, inputValidation = null) }
    }

    fun close() = synchronized(gate) {
        stopped = true
        retireActive()
        state.update { it.copy(validatingItemId = null, inputValidation = null) }
    }

    private fun retireActive() {
        val previous = active
        active = null
        previous?.job?.cancel()
    }

    private fun claim(expected: CreationNativeItem?): Attempt? = synchronized(gate) {
        if (stopped || closed() || active != null) return@synchronized null
        var current = state.value
        while (current.validatingItemId == null) {
            val snapshot = current.selectedItem ?: return@synchronized null
            if (!snapshot.isConfigurable() || expected != null && snapshot != expected) return@synchronized null
            if (state.compareAndSet(current, current.copy(validatingItemId = snapshot.id, inputValidation = null))) {
                return@synchronized Attempt(snapshot.copy(referencePaths = snapshot.referencePaths.toList())).also { active = it }
            }
            current = state.value
        }
        null
    }

    fun submit() { start(null) }

    private fun start(expected: CreationNativeItem?) {
        val attempt = claim(expected) ?: return
        val job = scope.launch(dispatcher) {
            try {
                if (synchronized(gate) { active !== attempt || stopped || closed() }) return@launch
                val failure = try { inspect(attempt.snapshot) }
                catch (cancelled: CancellationException) { throw cancelled }
                catch (_: Exception) { "validation_unavailable" }
                val committed = settle(attempt, failure)
                if (committed) synchronized(gate) { if (!stopped && !closed()) accepted() }
            } finally {
                synchronized(gate) {
                    if (active === attempt) {
                        active = null
                        state.update { if (it.validatingItemId == attempt.snapshot.id) it.copy(validatingItemId = null) else it }
                    }
                }
            }
        }
        synchronized(gate) { if (active === attempt) attempt.job = job else job.cancel() }
    }

    private fun settle(attempt: Attempt, failure: String?): Boolean = synchronized(gate) {
        while (active === attempt && !stopped) {
            val current = state.value
            val matches = !closed() && current.selectedItem == attempt.snapshot && current.validatingItemId == attempt.snapshot.id
            val submit = matches && failure == null
            val next = when {
                !matches -> if (current.validatingItemId == attempt.snapshot.id) current.copy(validatingItemId = null) else current
                failure != null -> current.copy(validatingItemId = null, inputValidation = CreationInputValidation(attempt.snapshot, failure))
                else -> current.submitSelectedItem().copy(validatingItemId = null)
            }
            if (state.compareAndSet(current, next)) { active = null; return@synchronized submit }
        }
        false
    }

}

internal fun inspectCreationInput(files: CreationFileStore, tool: CreationTool, sourcePath: String, generationMode: String): String? {
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

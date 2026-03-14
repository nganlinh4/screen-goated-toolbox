package dev.screengoated.toolbox.mobile.service.parakeet

sealed class ParakeetModelState {
    data object Missing : ParakeetModelState()
    data class Downloading(val progress: Float, val currentFile: String) : ParakeetModelState()
    data class Installed(val sizeBytes: Long) : ParakeetModelState()
    data object Deleting : ParakeetModelState()
    data class Error(val message: String) : ParakeetModelState()
}

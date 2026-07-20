package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

/** Flavor delivery result. The shared manager owns validation and finalization. */
internal sealed interface UiDetectorBundledModelResult {
    data object Unavailable : UiDetectorBundledModelResult
    data object Pending : UiDetectorBundledModelResult
    data object Copied : UiDetectorBundledModelResult
    data class Failed(val message: String) : UiDetectorBundledModelResult
}

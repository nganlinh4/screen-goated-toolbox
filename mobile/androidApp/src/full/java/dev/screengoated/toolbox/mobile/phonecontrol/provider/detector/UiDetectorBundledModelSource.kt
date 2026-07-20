package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

import android.content.Context
import java.io.File

/** Sideload builds retain their existing verified network-delivery contract. */
internal object UiDetectorBundledModelSource {
    suspend fun copyTo(
        context: Context,
        partial: File,
    ): UiDetectorBundledModelResult {
        return UiDetectorBundledModelResult.Unavailable
    }
}

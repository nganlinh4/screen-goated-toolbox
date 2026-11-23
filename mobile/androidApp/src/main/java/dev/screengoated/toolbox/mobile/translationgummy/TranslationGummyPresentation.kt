package dev.screengoated.toolbox.mobile.translationgummy

import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.sample

internal const val GUMMY_METER_FRAME_MS = 50L

@OptIn(FlowPreview::class)
internal class TranslationGummyPresentation(initial: TranslationGummyState) {
    val mutableState = MutableStateFlow(initial)
    val state = mutableState.asStateFlow()
    private val meter = MutableStateFlow(0f)
    val visualizerLevels = meter.sample(GUMMY_METER_FRAME_MS)
        .map { normalizedGummyMeterLevel(mutableState.value.isRunning, meter.value) }
        .distinctUntilChanged()

    fun updateVisualizerLevel(level: Float) {
        meter.value = normalizedGummyMeterLevel(mutableState.value.isRunning, level)
    }

    fun resetMeter() {
        meter.value = 0f
    }
}

internal fun normalizedGummyMeterLevel(running: Boolean, level: Float): Float =
    if (running && level.isFinite()) level.coerceIn(0f, 1f) else 0f

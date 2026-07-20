package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GenerationPlaybackChunk
import dev.screengoated.toolbox.mobile.shared.live.GenerationPlaybackGate

internal typealias LiveTranslatePlaybackChunk = GenerationPlaybackChunk
internal typealias LiveTranslatePlaybackEpoch = GenerationPlaybackGate

internal fun executeLiveTranslateFeatureEffects(
    effects: List<GeminiLiveLifecycleEffect>,
    deliverContent: (Int) -> Unit,
    stopPlayback: () -> Unit,
    discardQueuedOutput: () -> Unit,
) {
    effects.forEach { effect ->
        when (effect) {
            is GeminiLiveLifecycleEffect.DeliverContent -> deliverContent(effect.count)
            GeminiLiveLifecycleEffect.StopPlayback -> stopPlayback()
            GeminiLiveLifecycleEffect.DiscardQueuedOutput -> discardQueuedOutput()
            GeminiLiveLifecycleEffect.FinalizeGeneration,
            GeminiLiveLifecycleEffect.FinalizeTurn,
            GeminiLiveLifecycleEffect.FinalizeInterruptedGeneration,
            -> Unit
            is GeminiLiveLifecycleEffect.DispatchTools,
            is GeminiLiveLifecycleEffect.CancelTools,
            -> error("Live Translate does not support Gemini Live tool effects")
            is GeminiLiveLifecycleEffect.OpenSocket,
            is GeminiLiveLifecycleEffect.SendSetup,
            is GeminiLiveLifecycleEffect.CloseSocket,
            is GeminiLiveLifecycleEffect.ScheduleReconnect,
            is GeminiLiveLifecycleEffect.ReportFailure,
            GeminiLiveLifecycleEffect.CancelSession,
            is GeminiLiveLifecycleEffect.FinalizeResponse,
            -> error("Transport lifecycle effect escaped the Live Translate adapter")
        }
    }
}

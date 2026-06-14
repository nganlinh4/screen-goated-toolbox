package dev.screengoated.toolbox.mobile.model

/**
 * Centralized TTS defaults shared across the app.
 *
 * Mirrors how [dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog.DEFAULT_TTS_GEMINI_MODEL]
 * centralizes the default Gemini TTS model, so the default voice is defined in exactly one place.
 */
object TtsDefaults {
    /** Default prebuilt Gemini Live voice used when no explicit voice is selected. */
    const val DEFAULT_TTS_GEMINI_VOICE = "Aoede"
}

package dev.screengoated.toolbox.mobile.model

import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionPatch

/**
 * Live Translate part of the Windows restore_defaults "audio settings" scope
 * (src/gui/settings_ui/global/restore_defaults.rs): realtime models, vocabulary,
 * transcription and target languages, and audio source. Display mode, auth and
 * notification behavior are Android platform settings and are kept.
 */
internal fun liveTranslateRestorePatch(defaults: LiveSessionConfig = LiveSessionConfig()) = LiveSessionPatch(
    sourceMode = defaults.sourceMode,
    targetLanguage = defaults.targetLanguage,
    transcriptionLanguage = defaults.transcriptionLanguage,
    transcriptionProvider = defaults.transcriptionProvider,
    customVocabulary = defaults.customVocabulary,
    translationProvider = defaults.translationProvider,
)

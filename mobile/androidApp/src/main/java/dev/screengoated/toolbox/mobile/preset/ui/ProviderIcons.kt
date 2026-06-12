package dev.screengoated.toolbox.mobile.preset.ui

import androidx.annotation.DrawableRes
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider

@DrawableRes
internal fun providerIconRes(provider: PresetModelProvider): Int = when (provider) {
    PresetModelProvider.GOOGLE,
    PresetModelProvider.GEMINI_LIVE,
    -> R.drawable.ms_auto_awesome
    PresetModelProvider.GOOGLE_GTX -> R.drawable.ms_translate
    PresetModelProvider.GROQ -> R.drawable.ms_electric_bolt
    PresetModelProvider.CEREBRAS -> R.drawable.ms_local_fire_department
    PresetModelProvider.OPENROUTER -> R.drawable.ms_public
    PresetModelProvider.OLLAMA -> R.drawable.ms_terminal
    PresetModelProvider.QRSERVER -> R.drawable.ms_qr_code_scanner
    PresetModelProvider.PARAKEET -> R.drawable.ms_speech_to_text
    PresetModelProvider.MOONSHINE -> R.drawable.ms_graphic_eq
    PresetModelProvider.TAALAS -> R.drawable.ms_auto_awesome
}

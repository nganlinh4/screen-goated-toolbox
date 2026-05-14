@file:OptIn(androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui.ttssettings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.ui.ExpressiveDialogSectionCard

/**
 * Settings section for the five offline leaderboard TTS providers.
 */
@Composable
internal fun OpenWeightsSection(
    settings: MobileGlobalTtsSettings,
    method: MobileTtsMethod,
    onSettingsChanged: (MobileGlobalTtsSettings) -> Unit,
) {
    ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.primary) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            when (method) {
                MobileTtsMethod.KOKORO -> KokoroFields(settings, onSettingsChanged)
                MobileTtsMethod.STEP_AUDIO_EDITX -> DeferredNotice(
                    title = "Step Audio EditX",
                    detail = "Supports Mandarin, English, Sichuanese, Cantonese, Japanese, and Korean.",
                )
                MobileTtsMethod.MAGPIE_MULTILINGUAL -> DeferredNotice(
                    title = "NVIDIA Magpie-Multilingual 357M",
                    detail = "Supports English, Spanish, German, French, Vietnamese, Italian, Mandarin Chinese, Hindi, and Japanese.",
                )
                MobileTtsMethod.VOXTRAL_TTS -> DeferredNotice(
                    title = "Mistral Voxtral 4B TTS",
                    detail = "Supports English, French, Spanish, German, Italian, Portuguese, Dutch, Arabic, and Hindi.",
                )
                else -> Unit
            }
        }
    }
}

@Composable
private fun DeferredNotice(title: String, detail: String) {
    Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
    Text(
        "$detail Offline voice generation is not available for this model on Android yet.",
        color = Color(0xFFE49B0F),
        style = MaterialTheme.typography.bodySmall,
    )
}

@Composable
private fun KokoroFields(
    settings: MobileGlobalTtsSettings,
    onSettingsChanged: (MobileGlobalTtsSettings) -> Unit,
) {
    val s = settings.kokoroSettings
    Text("Kokoro 82M v1.0", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
    Text(
        "Supports English, Mandarin Chinese, Japanese, Spanish, French, Hindi, Italian, and Portuguese.",
        style = MaterialTheme.typography.bodySmall,
    )
    OutlinedTextField(
        modifier = Modifier.fillMaxWidth(),
        label = { Text("Voice") },
        value = s.voice,
        onValueChange = { v -> onSettingsChanged(settings.copy(kokoroSettings = s.copy(voice = v))) },
        placeholder = { Text("e.g. af_heart, am_adam, jf_alpha") },
        singleLine = true,
    )
    OutlinedTextField(
        modifier = Modifier.fillMaxWidth(),
        label = { Text("Language (BCP-47, optional)") },
        value = s.lang,
        onValueChange = { v -> onSettingsChanged(settings.copy(kokoroSettings = s.copy(lang = v))) },
        placeholder = { Text("en-us, ja, zh, es ...") },
        singleLine = true,
    )
    Text(
        "Speed: ${"%.2f".format(s.speed)}",
        style = MaterialTheme.typography.bodySmall,
    )
    Slider(
        value = s.speed,
        onValueChange = { v -> onSettingsChanged(settings.copy(kokoroSettings = s.copy(speed = v))) },
        valueRange = 0.5f..2.0f,
    )
    Text(
        "CPU threads: ${s.numThreads}",
        style = MaterialTheme.typography.bodySmall,
    )
    Slider(
        value = s.numThreads.toFloat(),
        onValueChange = { v ->
            onSettingsChanged(settings.copy(kokoroSettings = s.copy(numThreads = v.toInt().coerceIn(1, 8))))
        },
        valueRange = 1f..8f,
        steps = 6,
    )
}

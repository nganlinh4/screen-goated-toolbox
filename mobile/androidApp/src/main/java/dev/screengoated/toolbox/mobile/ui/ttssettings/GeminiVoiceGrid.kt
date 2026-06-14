@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui.ttssettings

import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.GeminiVoiceOption
import dev.screengoated.toolbox.mobile.model.MobileTtsCatalog
import dev.screengoated.toolbox.mobile.ui.ExpressiveDialogSectionCard
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun GeminiVoiceGrid(
    selectedVoice: String,
    locale: MobileLocaleText,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        BoxWithConstraints {
            when {
                maxWidth >= 900.dp -> FourColumnVoiceGrid(selectedVoice, locale, onVoiceChanged, onPreviewVoice)
                maxWidth >= 600.dp -> TwoColumnVoiceGrid(selectedVoice, locale, onVoiceChanged, onPreviewVoice)
                else -> SingleColumnVoiceGrid(selectedVoice, locale, onVoiceChanged, onPreviewVoice)
            }
        }
    }
}

@Composable
private fun FourColumnVoiceGrid(
    selectedVoice: String,
    locale: MobileLocaleText,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
) {
    val maleVoices = MobileTtsCatalog.maleVoices
    val femaleVoices = MobileTtsCatalog.femaleVoices
    val maleMid = maleVoices.size.divCeil(2)
    val femaleMid = femaleVoices.size.divCeil(2)

    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        VoiceColumnCard(
            title = locale.ttsMale,
            voices = maleVoices.take(maleMid),
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
            modifier = Modifier.weight(1f),
        )
        VoiceColumnCard(
            title = null,
            voices = maleVoices.drop(maleMid),
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
            modifier = Modifier.weight(1f),
        )
        VoiceColumnCard(
            title = locale.ttsFemale,
            voices = femaleVoices.take(femaleMid),
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
            modifier = Modifier.weight(1f),
        )
        VoiceColumnCard(
            title = null,
            voices = femaleVoices.drop(femaleMid),
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun TwoColumnVoiceGrid(
    selectedVoice: String,
    locale: MobileLocaleText,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        VoiceColumnCard(
            title = locale.ttsMale,
            voices = MobileTtsCatalog.maleVoices,
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
            modifier = Modifier.weight(1f),
        )
        VoiceColumnCard(
            title = locale.ttsFemale,
            voices = MobileTtsCatalog.femaleVoices,
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun SingleColumnVoiceGrid(
    selectedVoice: String,
    locale: MobileLocaleText,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        VoiceColumnCard(
            title = locale.ttsMale,
            voices = MobileTtsCatalog.maleVoices,
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
        )
        VoiceColumnCard(
            title = locale.ttsFemale,
            voices = MobileTtsCatalog.femaleVoices,
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
        )
    }
}

@Composable
private fun VoiceColumnCard(
    title: String?,
    voices: List<GeminiVoiceOption>,
    selectedVoice: String,
    locale: MobileLocaleText,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val accent = if (title == locale.ttsMale) {
        MaterialTheme.colorScheme.primary
    } else {
        MaterialTheme.colorScheme.secondary
    }

    ExpressiveDialogSectionCard(
        accent = accent,
        modifier = modifier,
    ) {
        if (title != null) {
            Text(
                text = title,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.SemiBold,
                color = accent,
            )
        }
        voices.forEach { voice ->
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                RadioButton(
                    selected = selectedVoice == voice.name,
                    onClick = { onVoiceChanged(voice.name) },
                )
                IconButton(
                    onClick = {
                        onVoiceChanged(voice.name)
                        onPreviewVoice(voice.name)
                    },
                ) {
                    Icon(
                        painterResource(R.drawable.ms_volume_up),
                        contentDescription = locale.ttsVoiceLabel,
                    )
                }
                Text(
                    text = voice.name,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Medium,
                )
            }
        }
    }
}

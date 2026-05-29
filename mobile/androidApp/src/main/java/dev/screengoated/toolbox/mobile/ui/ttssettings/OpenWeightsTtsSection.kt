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
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

/**
 * Settings section for Windows-parity open-weight TTS providers.
 */
@Composable
internal fun OpenWeightsSection(
    settings: MobileGlobalTtsSettings,
    method: MobileTtsMethod,
    locale: MobileLocaleText,
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
                MobileTtsMethod.KOKORO -> KokoroFields(settings, locale, onSettingsChanged)
                MobileTtsMethod.STEP_AUDIO_EDITX -> DeferredNotice(
                    copy = openWeightTtsCopy(method, locale),
                )
                MobileTtsMethod.MAGPIE_MULTILINGUAL -> DeferredNotice(
                    copy = openWeightTtsCopy(method, locale),
                )
                MobileTtsMethod.SUPERTONIC -> DeferredNotice(
                    copy = openWeightTtsCopy(method, locale),
                )
                MobileTtsMethod.VIENEU_TTS -> DeferredNotice(
                    copy = openWeightTtsCopy(method, locale),
                )
                MobileTtsMethod.VOXTRAL_TTS -> DeferredNotice(
                    copy = openWeightTtsCopy(method, locale),
                )
                else -> Unit
            }
        }
    }
}

@Composable
private fun DeferredNotice(copy: OpenWeightTtsCopy) {
    Text(copy.title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
    Text(
        "${copy.detail} ${copy.unavailable}",
        color = Color(0xFFE49B0F),
        style = MaterialTheme.typography.bodySmall,
    )
}

@Composable
private fun KokoroFields(
    settings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    onSettingsChanged: (MobileGlobalTtsSettings) -> Unit,
) {
    val s = settings.kokoroSettings
    val copy = openWeightTtsCopy(MobileTtsMethod.KOKORO, locale)
    Text(copy.title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
    Text(
        copy.detail,
        style = MaterialTheme.typography.bodySmall,
    )
    OutlinedTextField(
        modifier = Modifier.fillMaxWidth(),
        label = { Text(locale.ttsVoiceLabel.trimEnd(':')) },
        value = s.voice,
        onValueChange = { v -> onSettingsChanged(settings.copy(kokoroSettings = s.copy(voice = v))) },
        placeholder = { Text("e.g. af_heart, am_adam, jf_alpha") },
        singleLine = true,
    )
    OutlinedTextField(
        modifier = Modifier.fillMaxWidth(),
        label = { Text(openWeightLanguageOptionalLabel(locale).trimEnd(':')) },
        value = s.lang,
        onValueChange = { v -> onSettingsChanged(settings.copy(kokoroSettings = s.copy(lang = v))) },
        placeholder = { Text("en-us, ja, zh, es ...") },
        singleLine = true,
    )
    Text(
        "${locale.ttsSpeedLabel} ${"%.2f".format(s.speed)}",
        style = MaterialTheme.typography.bodySmall,
    )
    Slider(
        value = s.speed,
        onValueChange = { v -> onSettingsChanged(settings.copy(kokoroSettings = s.copy(speed = v))) },
        valueRange = 0.5f..2.0f,
    )
    Text(
        "${openWeightCpuThreadsLabel(locale)} ${s.numThreads}",
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

internal data class OpenWeightTtsCopy(
    val title: String,
    val detail: String,
    val unavailable: String,
)

internal fun openWeightTtsCopy(
    method: MobileTtsMethod,
    locale: MobileLocaleText,
): OpenWeightTtsCopy {
    val title = when (method) {
        MobileTtsMethod.KOKORO -> "Kokoro 82M v1.0"
        MobileTtsMethod.STEP_AUDIO_EDITX -> "Step Audio EditX"
        MobileTtsMethod.MAGPIE_MULTILINGUAL -> "NVIDIA Magpie-Multilingual 357M"
        MobileTtsMethod.SUPERTONIC -> "Supertonic 3"
        MobileTtsMethod.VIENEU_TTS -> "VieNeu-TTS v2"
        MobileTtsMethod.VOXTRAL_TTS -> "Mistral Voxtral 4B TTS"
        else -> method.name
    }
    val detail = when (method) {
        MobileTtsMethod.KOKORO -> openWeightKokoroSupport(locale)
        MobileTtsMethod.STEP_AUDIO_EDITX -> openWeightStepSupport(locale)
        MobileTtsMethod.MAGPIE_MULTILINGUAL -> openWeightMagpieSupport(locale)
        MobileTtsMethod.SUPERTONIC -> openWeightSupertonicSupport(locale)
        MobileTtsMethod.VIENEU_TTS -> openWeightVieneuSupport(locale)
        MobileTtsMethod.VOXTRAL_TTS -> openWeightVoxtralSupport(locale)
        else -> ""
    }
    return OpenWeightTtsCopy(
        title = title,
        detail = detail,
        unavailable = openWeightAndroidUnavailable(locale),
    )
}

internal fun openWeightLanguageOptionalLabel(locale: MobileLocaleText): String = when (openWeightLocaleKey(locale)) {
    "vi" -> "Ngôn ngữ (BCP-47, tùy chọn):"
    "ko" -> "언어 (BCP-47, 선택 사항):"
    else -> "Language (BCP-47, optional):"
}

private fun openWeightCpuThreadsLabel(locale: MobileLocaleText): String = when (openWeightLocaleKey(locale)) {
    "vi" -> "Luồng CPU:"
    "ko" -> "CPU 스레드:"
    else -> "CPU threads:"
}

private fun openWeightAndroidUnavailable(locale: MobileLocaleText): String = when (openWeightLocaleKey(locale)) {
    "vi" -> "Tạo giọng offline cho model này chưa có trên Android."
    "ko" -> "이 모델의 오프라인 음성 생성은 아직 Android에서 사용할 수 없습니다."
    else -> "Offline voice generation is not available for this model on Android yet."
}

private fun openWeightKokoroSupport(locale: MobileLocaleText): String = when (openWeightLocaleKey(locale)) {
    "vi" -> "Hỗ trợ tiếng Anh, Quan thoại, Nhật, Tây Ban Nha, Pháp, Hindi, Ý và Bồ Đào Nha."
    "ko" -> "영어, 중국어(만다린), 일본어, 스페인어, 프랑스어, 힌디어, 이탈리아어, 포르투갈어를 지원합니다."
    else -> "Supports English, Mandarin Chinese, Japanese, Spanish, French, Hindi, Italian, and Portuguese."
}

private fun openWeightStepSupport(locale: MobileLocaleText): String = when (openWeightLocaleKey(locale)) {
    "vi" -> "Hỗ trợ Quan thoại, tiếng Anh, Tứ Xuyên, Quảng Đông, Nhật và Hàn."
    "ko" -> "중국어(만다린), 영어, 쓰촨어, 광둥어, 일본어, 한국어를 지원합니다."
    else -> "Supports Mandarin, English, Sichuanese, Cantonese, Japanese, and Korean."
}

private fun openWeightMagpieSupport(locale: MobileLocaleText): String = when (openWeightLocaleKey(locale)) {
    "vi" -> "Hỗ trợ tiếng Anh, Tây Ban Nha, Đức, Pháp, Việt, Ý, Quan thoại, Hindi và Nhật."
    "ko" -> "영어, 스페인어, 독일어, 프랑스어, 베트남어, 이탈리아어, 중국어(만다린), 힌디어, 일본어를 지원합니다."
    else -> "Supports English, Spanish, German, French, Vietnamese, Italian, Mandarin Chinese, Hindi, and Japanese."
}

private fun openWeightSupertonicSupport(locale: MobileLocaleText): String = when (openWeightLocaleKey(locale)) {
    "vi" -> "Hỗ trợ tiếng Anh, Tây Ban Nha, Pháp, Đức, Ý, Bồ Đào Nha, Ba Lan, Thổ Nhĩ Kỳ, Nga, Hà Lan, Séc, Ả Rập, Quan thoại, Nhật, Hungary, Hàn và Hindi."
    "ko" -> "영어, 스페인어, 프랑스어, 독일어, 이탈리아어, 포르투갈어, 폴란드어, 터키어, 러시아어, 네덜란드어, 체코어, 아랍어, 중국어(만다린), 일본어, 헝가리어, 한국어, 힌디어를 지원합니다."
    else -> "Supports English, Spanish, French, German, Italian, Portuguese, Polish, Turkish, Russian, Dutch, Czech, Arabic, Mandarin Chinese, Japanese, Hungarian, Korean, and Hindi."
}

private fun openWeightVieneuSupport(locale: MobileLocaleText): String = when (openWeightLocaleKey(locale)) {
    "vi" -> "TTS local ưu tiên tiếng Việt, hỗ trợ trộn Anh/Việt và clone giọng zero-shot."
    "ko" -> "베트남어 우선 로컬 TTS이며 영어/베트남어 코드 스위칭과 제로샷 음성 복제를 지원합니다."
    else -> "Vietnamese-first local TTS with English/Vietnamese code-switching and zero-shot voice cloning."
}

private fun openWeightVoxtralSupport(locale: MobileLocaleText): String = when (openWeightLocaleKey(locale)) {
    "vi" -> "Hỗ trợ tiếng Anh, Pháp, Tây Ban Nha, Đức, Ý, Bồ Đào Nha, Hà Lan, Ả Rập và Hindi."
    "ko" -> "영어, 프랑스어, 스페인어, 독일어, 이탈리아어, 포르투갈어, 네덜란드어, 아랍어, 힌디어를 지원합니다."
    else -> "Supports English, French, Spanish, German, Italian, Portuguese, Dutch, Arabic, and Hindi."
}

private fun openWeightLocaleKey(locale: MobileLocaleText): String = when (locale.ttsPreviewAction) {
    "Nghe thử" -> "vi"
    "미리 듣기" -> "ko"
    else -> "en"
}

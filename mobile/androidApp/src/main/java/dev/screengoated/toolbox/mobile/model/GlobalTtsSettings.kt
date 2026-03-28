package dev.screengoated.toolbox.mobile.model

import dev.screengoated.toolbox.mobile.shared.live.GeneratedGeminiLiveModelOption
import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog
import kotlinx.serialization.Serializable
import java.util.Locale

@Serializable
enum class MobileTtsMethod {
    GEMINI_LIVE,
    EDGE_TTS,
    GOOGLE_TRANSLATE,
}

@Serializable
enum class MobileTtsSpeedPreset {
    SLOW,
    NORMAL,
    FAST,
}

@Serializable
data class MobileTtsLanguageCondition(
    val languageCode: String,
    val languageName: String,
    val instruction: String,
)

@Serializable
data class MobileEdgeTtsVoiceConfig(
    val languageCode: String,
    val languageName: String,
    val voiceName: String,
)

@Serializable
data class MobileEdgeTtsSettings(
    val pitch: Int = 0,
    val rate: Int = 0,
    val volume: Int = 0,
    val voiceConfigs: List<MobileEdgeTtsVoiceConfig> = defaultEdgeTtsVoiceConfigs(),
)

@Serializable
data class MobileGlobalTtsSettings(
    val method: MobileTtsMethod = MobileTtsMethod.GEMINI_LIVE,
    val geminiModel: String = GeneratedLiveModelCatalog.DEFAULT_TTS_GEMINI_MODEL,
    val voice: String = "Aoede",
    val speedPreset: MobileTtsSpeedPreset = MobileTtsSpeedPreset.FAST,
    val languageConditions: List<MobileTtsLanguageCondition> = defaultTtsLanguageConditions(),
    val edgeSettings: MobileEdgeTtsSettings = MobileEdgeTtsSettings(),
)

fun MobileGlobalTtsSettings.withMethod(method: MobileTtsMethod): MobileGlobalTtsSettings {
    val coercedSpeed = if (method == MobileTtsMethod.GOOGLE_TRANSLATE && speedPreset == MobileTtsSpeedPreset.FAST) {
        MobileTtsSpeedPreset.NORMAL
    } else {
        speedPreset
    }
    return copy(method = method, speedPreset = coercedSpeed)
}

data class GeminiVoiceOption(
    val name: String,
    val gender: String,
)

data class GeminiLiveModelOption(
    val apiModel: String,
    val label: String,
)

data class TtsLanguageOption(
    val code: String,
    val name: String,
)

fun defaultTtsLanguageConditions(): List<MobileTtsLanguageCondition> {
    return listOf(
        MobileTtsLanguageCondition(
            languageCode = "vie",
            languageName = "Vietnamese",
            instruction = "Speak in a \"giọng miền Tây\" accent.",
        ),
    )
}

fun defaultEdgeTtsVoiceConfigs(): List<MobileEdgeTtsVoiceConfig> {
    return listOf(
        MobileEdgeTtsVoiceConfig("en", "English", "en-US-AriaNeural"),
        MobileEdgeTtsVoiceConfig("vi", "Vietnamese", "vi-VN-HoaiMyNeural"),
        MobileEdgeTtsVoiceConfig("ko", "Korean", "ko-KR-SunHiNeural"),
        MobileEdgeTtsVoiceConfig("ja", "Japanese", "ja-JP-NanamiNeural"),
        MobileEdgeTtsVoiceConfig("zh", "Chinese", "zh-CN-XiaoxiaoNeural"),
    )
}

object MobileTtsCatalog {
    val geminiModels: List<GeminiLiveModelOption> =
        GeneratedLiveModelCatalog.ttsGeminiModels.map(GeneratedGeminiLiveModelOption::toAppOption)

    val maleVoices: List<GeminiVoiceOption> = listOf(
        GeminiVoiceOption("Achird", "Male"),
        GeminiVoiceOption("Algenib", "Male"),
        GeminiVoiceOption("Algieba", "Male"),
        GeminiVoiceOption("Alnilam", "Male"),
        GeminiVoiceOption("Charon", "Male"),
        GeminiVoiceOption("Enceladus", "Male"),
        GeminiVoiceOption("Fenrir", "Male"),
        GeminiVoiceOption("Iapetus", "Male"),
        GeminiVoiceOption("Orus", "Male"),
        GeminiVoiceOption("Puck", "Male"),
        GeminiVoiceOption("Rasalgethi", "Male"),
        GeminiVoiceOption("Sadachbia", "Male"),
        GeminiVoiceOption("Sadaltager", "Male"),
        GeminiVoiceOption("Schedar", "Male"),
        GeminiVoiceOption("Umbriel", "Male"),
        GeminiVoiceOption("Zubenelgenubi", "Male"),
    )

    val femaleVoices: List<GeminiVoiceOption> = listOf(
        GeminiVoiceOption("Achernar", "Female"),
        GeminiVoiceOption("Aoede", "Female"),
        GeminiVoiceOption("Autonoe", "Female"),
        GeminiVoiceOption("Callirrhoe", "Female"),
        GeminiVoiceOption("Despina", "Female"),
        GeminiVoiceOption("Erinome", "Female"),
        GeminiVoiceOption("Gacrux", "Female"),
        GeminiVoiceOption("Kore", "Female"),
        GeminiVoiceOption("Laomedeia", "Female"),
        GeminiVoiceOption("Leda", "Female"),
        GeminiVoiceOption("Pulcherrima", "Female"),
        GeminiVoiceOption("Sulafat", "Female"),
        GeminiVoiceOption("Vindemiatrix", "Female"),
        GeminiVoiceOption("Zephyr", "Female"),
    )

    val conditionLanguages: List<TtsLanguageOption> = listOf(
        TtsLanguageOption("afr", "Afrikaans"),
        TtsLanguageOption("ara", "Arabic"),
        TtsLanguageOption("aze", "Azerbaijani"),
        TtsLanguageOption("bel", "Belarusian"),
        TtsLanguageOption("ben", "Bengali"),
        TtsLanguageOption("bul", "Bulgarian"),
        TtsLanguageOption("cat", "Catalan"),
        TtsLanguageOption("ces", "Czech"),
        TtsLanguageOption("cmn", "Mandarin Chinese"),
        TtsLanguageOption("dan", "Danish"),
        TtsLanguageOption("deu", "German"),
        TtsLanguageOption("ell", "Greek"),
        TtsLanguageOption("eng", "English"),
        TtsLanguageOption("epo", "Esperanto"),
        TtsLanguageOption("est", "Estonian"),
        TtsLanguageOption("eus", "Basque"),
        TtsLanguageOption("fin", "Finnish"),
        TtsLanguageOption("fra", "French"),
        TtsLanguageOption("guj", "Gujarati"),
        TtsLanguageOption("heb", "Hebrew"),
        TtsLanguageOption("hin", "Hindi"),
        TtsLanguageOption("hrv", "Croatian"),
        TtsLanguageOption("hun", "Hungarian"),
        TtsLanguageOption("ind", "Indonesian"),
        TtsLanguageOption("ita", "Italian"),
        TtsLanguageOption("jpn", "Japanese"),
        TtsLanguageOption("kan", "Kannada"),
        TtsLanguageOption("kat", "Georgian"),
        TtsLanguageOption("kor", "Korean"),
        TtsLanguageOption("lat", "Latin"),
        TtsLanguageOption("lav", "Latvian"),
        TtsLanguageOption("lit", "Lithuanian"),
        TtsLanguageOption("mal", "Malayalam"),
        TtsLanguageOption("mar", "Marathi"),
        TtsLanguageOption("mkd", "Macedonian"),
        TtsLanguageOption("mya", "Burmese"),
        TtsLanguageOption("nep", "Nepali"),
        TtsLanguageOption("nld", "Dutch"),
        TtsLanguageOption("nno", "Norwegian Nynorsk"),
        TtsLanguageOption("nob", "Norwegian Bokmal"),
        TtsLanguageOption("ori", "Oriya"),
        TtsLanguageOption("pan", "Punjabi"),
        TtsLanguageOption("pes", "Persian"),
        TtsLanguageOption("pol", "Polish"),
        TtsLanguageOption("por", "Portuguese"),
        TtsLanguageOption("ron", "Romanian"),
        TtsLanguageOption("rus", "Russian"),
        TtsLanguageOption("sin", "Sinhala"),
        TtsLanguageOption("slk", "Slovak"),
        TtsLanguageOption("slv", "Slovenian"),
        TtsLanguageOption("som", "Somali"),
        TtsLanguageOption("spa", "Spanish"),
        TtsLanguageOption("sqi", "Albanian"),
        TtsLanguageOption("srp", "Serbian"),
        TtsLanguageOption("swe", "Swedish"),
        TtsLanguageOption("tam", "Tamil"),
        TtsLanguageOption("tel", "Telugu"),
        TtsLanguageOption("tgl", "Tagalog"),
        TtsLanguageOption("tha", "Thai"),
        TtsLanguageOption("tur", "Turkish"),
        TtsLanguageOption("ukr", "Ukrainian"),
        TtsLanguageOption("urd", "Urdu"),
        TtsLanguageOption("uzb", "Uzbek"),
        TtsLanguageOption("vie", "Vietnamese"),
        TtsLanguageOption("yid", "Yiddish"),
        TtsLanguageOption("zho", "Chinese"),
    )

    val edgeConfigLanguages: List<TtsLanguageOption> = LanguageCatalog.names
        .map { name ->
            TtsLanguageOption(
                code = LanguageCatalog.codeForName(name).lowercase(Locale.US),
                name = name,
            )
        }
        .distinctBy { it.code }
        .sortedBy { it.name }

    fun edgeVoiceSuggestions(languageCode: String): List<String> {
        return edgeVoiceSuggestionsByLanguage[languageCode.lowercase(Locale.US)].orEmpty()
    }

    private val edgeVoiceSuggestionsByLanguage: Map<String, List<String>> = mapOf(
        "en" to listOf(
            "en-US-AriaNeural",
            "en-US-JennyNeural",
            "en-US-GuyNeural",
            "en-GB-SoniaNeural",
            "en-GB-RyanNeural",
        ),
        "vi" to listOf(
            "vi-VN-HoaiMyNeural",
            "vi-VN-NamMinhNeural",
        ),
        "ko" to listOf(
            "ko-KR-SunHiNeural",
            "ko-KR-InJoonNeural",
        ),
        "ja" to listOf(
            "ja-JP-NanamiNeural",
            "ja-JP-KeitaNeural",
        ),
        "zh" to listOf(
            "zh-CN-XiaoxiaoNeural",
            "zh-CN-YunxiNeural",
            "zh-TW-HsiaoChenNeural",
            "zh-TW-YunJheNeural",
        ),
        "de" to listOf(
            "de-DE-KatjaNeural",
            "de-DE-ConradNeural",
        ),
        "fr" to listOf(
            "fr-FR-DeniseNeural",
            "fr-FR-HenriNeural",
        ),
        "es" to listOf(
            "es-ES-ElviraNeural",
            "es-ES-AlvaroNeural",
        ),
    )

}

private fun GeneratedGeminiLiveModelOption.toAppOption(): GeminiLiveModelOption {
    return GeminiLiveModelOption(
        apiModel = apiModel,
        label = label,
    )
}

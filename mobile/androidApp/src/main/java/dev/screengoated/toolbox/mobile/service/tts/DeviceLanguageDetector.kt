package dev.screengoated.toolbox.mobile.service.tts

import android.content.Context
import android.view.textclassifier.TextClassificationManager
import android.view.textclassifier.TextLanguage
import java.util.Locale

class DeviceLanguageDetector(
    context: Context,
) {
    private val appContext = context.applicationContext

    fun detectIso639_3(text: String): String {
        return detectLanguageTag(text)
            ?.toIso639_3()
            ?: fallbackIso639_3(text)
    }

    fun detectIso639_1(text: String): String {
        return detectLanguageTag(text)
            ?.toIso639_1()
            ?: fallbackIso639_1(text)
    }

    private fun detectLanguageTag(text: String): String? {
        if (text.isBlank()) {
            return null
        }
        return runCatching {
            val manager = appContext.getSystemService(TextClassificationManager::class.java)
            val classifier = manager?.textClassifier ?: return@runCatching null
            val result = classifier.detectLanguage(TextLanguage.Request.Builder(text).build())
            if (result.localeHypothesisCount <= 0) {
                null
            } else {
                result.getLocale(0)?.toLanguageTag()
            }
        }.getOrNull()
    }

    private fun String.toIso639_1(): String {
        val locale = Locale.forLanguageTag(this)
        val language = locale.language?.lowercase(Locale.US).orEmpty()
        return if (language.isNotBlank()) {
            language
        } else {
            fallbackIso639_1(this)
        }
    }

    private fun String.toIso639_3(): String {
        val iso1 = toIso639_1()
        return ISO1_TO_ISO3[iso1] ?: fallbackIso639_3(this)
    }

    private fun fallbackIso639_1(text: String): String {
        val sample = text.lowercase(Locale.US)
        return when {
            sample.any(::isHangul) -> "ko"
            sample.any(::isJapaneseKana) -> "ja"
            sample.any(::isCjkUnifiedIdeograph) -> "zh"
            sample.any(::isCyrillic) -> "ru"
            sample.any { it in VIETNAMESE_MARKERS } -> "vi"
            else -> "en"
        }
    }

    private fun fallbackIso639_3(text: String): String {
        return ISO1_TO_ISO3[fallbackIso639_1(text)] ?: "eng"
    }

    private companion object {
        private val ISO1_TO_ISO3: Map<String, String> = buildMap {
            Locale.getISOLanguages().forEach { iso1 ->
                runCatching {
                    put(
                        iso1.lowercase(Locale.US),
                        Locale.forLanguageTag(iso1).isO3Language.lowercase(Locale.US),
                    )
                }
            }
        }

        private val VIETNAMESE_MARKERS = setOf(
            'ă',
            'â',
            'đ',
            'ê',
            'ô',
            'ơ',
            'ư',
        )

        private fun isHangul(char: Char): Boolean {
            val code = char.code
            return code in 0x1100..0x11FF || code in 0x3130..0x318F || code in 0xAC00..0xD7AF
        }

        private fun isJapaneseKana(char: Char): Boolean {
            val code = char.code
            return code in 0x3040..0x30FF || code in 0x31F0..0x31FF
        }

        private fun isCjkUnifiedIdeograph(char: Char): Boolean {
            val code = char.code
            return code in 0x4E00..0x9FFF
        }

        private fun isCyrillic(char: Char): Boolean {
            val code = char.code
            return code in 0x0400..0x04FF || code in 0x0500..0x052F
        }
    }
}

package dev.screengoated.toolbox.mobile.ui.i18n

import java.util.Locale

internal fun apiKeyErrorToastText(error: String, uiLanguage: String): String? {
    val normalized = error.trim()
    if (!normalized.contains("NO_API_KEY") && !normalized.contains("INVALID_API_KEY")) {
        return null
    }

    val key = listOf("NO_API_KEY:", "INVALID_API_KEY:").firstNotNullOfOrNull { marker ->
        normalized.indexOf(marker).takeIf { it >= 0 }?.let { index ->
            normalized.substring(index + marker.length).trimStart()
                .takeWhile { it.isLetterOrDigit() || it == '_' || it == '-' || it == '.' }
                .takeIf { it.isNotEmpty() }?.lowercase(Locale.ROOT)
        }
    }
    val provider = when (key) {
        "groq" -> "Groq"
        "openrouter" -> "OpenRouter"
        "nvidia" -> "NVIDIA"
        "openai" -> "OpenAI"
        "anthropic" -> "Anthropic"
        "google", "gemini", "gemini-live" -> "Gemini"
        null -> "API"
        else -> key
    }

    val credential = if (key == null) "API" else "$provider API"
    val article = if (key == null) "an" else "a"
    return when {
        normalized.contains("NO_API_KEY") -> when (uiLanguage) {
            "vi" -> "Bạn chưa nhập ${credential} key!"
            "ko" -> "${credential} 키를 입력하지 않았습니다!"
            "ja" -> "${credential}キーが入力されていません!"
            "zh" -> "您还没有输入 ${credential} key!"
            else -> "You haven't entered $article ${credential} key!"
        }

        else -> when (uiLanguage) {
            "vi" -> "${credential} key không hợp lệ!"
            "ko" -> "${credential} 키가 유효하지 않습니다!"
            "ja" -> "${credential}キーが無効です!"
            "zh" -> "${credential} key 无效!"
            else -> "Invalid ${credential} key!"
        }
    }
}

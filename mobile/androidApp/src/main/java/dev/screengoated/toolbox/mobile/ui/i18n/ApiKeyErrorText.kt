package dev.screengoated.toolbox.mobile.ui.i18n

internal fun apiKeyErrorToastText(error: String, uiLanguage: String): String? {
    val normalized = error.trim()
    if (!normalized.contains("NO_API_KEY") && !normalized.contains("INVALID_API_KEY")) {
        return null
    }

    val provider = when {
        normalized.contains("groq", ignoreCase = true) -> "Groq"
        normalized.contains("openrouter", ignoreCase = true) -> "OpenRouter"
        normalized.contains("cerebras", ignoreCase = true) -> "Cerebras"
        normalized.contains("openai", ignoreCase = true) -> "OpenAI"
        normalized.contains("google", ignoreCase = true) || normalized.contains("gemini", ignoreCase = true) -> "Google Gemini"
        else -> "API"
    }

    return when {
        normalized.contains("NO_API_KEY") -> when (uiLanguage) {
            "vi" -> "Bạn chưa nhập ${provider} API key!"
            "ko" -> "${provider} API 키를 입력하지 않았습니다!"
            "ja" -> "${provider} APIキーが入力されていません!"
            "zh" -> "您还没有输入 ${provider} API key!"
            else -> "You haven't entered a ${provider} API key!"
        }

        else -> when (uiLanguage) {
            "vi" -> "${provider} API key không hợp lệ!"
            "ko" -> "${provider} API 키가 유효하지 않습니다!"
            "ja" -> "${provider} APIキーが無効です!"
            "zh" -> "${provider} API key 无效!"
            else -> "Invalid ${provider} API key!"
        }
    }
}

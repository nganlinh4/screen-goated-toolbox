package dev.screengoated.toolbox.mobile.ui

internal enum class CredentialsProviderId(val label: String) {
    GROQ("Groq"),
    CEREBRAS("Cerebras"),
    GEMINI("Gemini"),
    OPEN_ROUTER("OpenRouter"),
    OLLAMA("Ollama"),
}

internal fun credentialsProviderOrder(): List<CredentialsProviderId> = listOf(
    CredentialsProviderId.GROQ,
    CredentialsProviderId.CEREBRAS,
    CredentialsProviderId.GEMINI,
    CredentialsProviderId.OPEN_ROUTER,
    CredentialsProviderId.OLLAMA,
)

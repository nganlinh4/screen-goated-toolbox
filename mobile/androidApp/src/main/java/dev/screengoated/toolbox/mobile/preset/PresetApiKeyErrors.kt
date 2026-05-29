package dev.screengoated.toolbox.mobile.preset

import java.util.Locale

internal fun providerApiKeyId(providerName: String): String =
    providerName
        .trim()
        .lowercase(Locale.ROOT)
        .filter { it.isLetterOrDigit() }
        .ifBlank { "api" }

internal fun invalidApiKeyMessage(providerName: String): String =
    "INVALID_API_KEY:${providerApiKeyId(providerName)}"

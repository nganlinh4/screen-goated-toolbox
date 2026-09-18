package dev.screengoated.toolbox.mobile.preset

import org.json.JSONObject

/** Retry an oversized reservation once at the provider's reported output allowance. */
internal fun groqOutputAllowanceRetry(status: Int, retried: Boolean, body: String, current: Long?): Int? {
    if (status != 429 || retried) return null
    val allowance = groqRequestAllowance(status, body) ?: return null
    return allowance.second.takeIf { allowance.first == "OTPM" && (current == null || current > it) }
}

internal fun groqRequestLimitPrefix(status: Int, body: String): String =
    if (groqRequestAllowance(status, body) != null) "PROVIDER_REQUEST_LIMIT:" else ""

private fun groqRequestAllowance(status: Int, body: String): Pair<String, Int>? {
    if (status != 429) return null
    val error = runCatching { JSONObject(body).getJSONObject("error") }.getOrNull() ?: return null
    if (error.optString("code") != "rate_limit_exceeded" || error.optString("type") != "tokens") return null
    val sections = error.optString("message").split("): Limit ", limit = 2)
    if (sections.size != 2) return null
    val kind = sections[0].substringAfterLast('(')
    if (kind !in setOf("OTPM", "ITPM", "TPM")) return null
    val fields = sections[1]
    val parts = fields.split(", Requested ", limit = 2)
    if (parts.size != 2) return null
    val limit = parts[0].toIntOrNull() ?: return null
    val requested = parts[1].substringBefore('.').toLongOrNull() ?: return null
    return (kind to limit).takeIf { limit > 0 && requested > limit }
}

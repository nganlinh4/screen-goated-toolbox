package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

internal data class ResearchRequest(
    val query: String,
    val purpose: String,
    val policy: SourcePolicy,
    val allowedDomains: List<String>,
    val sourceUrls: List<HttpUrl>,
    val maxSources: Int,
) {
    fun discoveryQuery(): String = when (policy) {
        SourcePolicy.DOMAIN_RESTRICTED ->
            "$query ${allowedDomains.joinToString(" ") { "site:$it" }}".take(MAX_EFFECTIVE_QUERY_CHARS)
        else -> query
    }

    fun effectiveQuery(): String = "$query $purpose".trim().take(MAX_EFFECTIVE_QUERY_CHARS)

    fun accepts(url: HttpUrl): Boolean =
        policy != SourcePolicy.DOMAIN_RESTRICTED ||
            allowedDomains.any { domain ->
                url.host.equals(domain, ignoreCase = true) ||
                    url.host.endsWith(".$domain", ignoreCase = true)
            }
}

internal enum class SourcePolicy(val wireName: String) {
    BEST_AVAILABLE("best_available"),
    BROAD("broad"),
    DOMAIN_RESTRICTED("domain_restricted"),
}

internal sealed interface ResearchRequestResult {
    data class Ready(val request: ResearchRequest) : ResearchRequestResult
    data class Failure(val outcome: PublicResearchOutcome) : ResearchRequestResult
}

internal fun parseResearchRequest(args: JsonObject): ResearchRequestResult {
    val query = args.string("query")?.trim().orEmpty()
    if (query.isBlank() || query.length > MAX_QUERY_CHARS) {
        return invalidResearch("research_query_invalid", "query is blank or too long.")
    }
    val purpose = args.string("purpose")?.trim().orEmpty()
    if (purpose.isBlank() || purpose.length > MAX_PURPOSE_CHARS) {
        return invalidResearch("research_purpose_invalid", "purpose is blank or too long.")
    }
    val policy = args.string("source_policy")?.let { wire ->
        SourcePolicy.entries.firstOrNull { it.wireName == wire }
    } ?: SourcePolicy.BEST_AVAILABLE
    val rawAllowedDomains = args.stringArray("allowed_domains")
    val allowedDomains = rawAllowedDomains
        ?.mapNotNull(::normalizeDomain)
        ?.distinct()
        .orEmpty()
    if (rawAllowedDomains != null && rawAllowedDomains.size != allowedDomains.size) {
        return invalidResearch(
            "research_domains_invalid",
            "allowed_domains must contain unique valid public hosts.",
        )
    }
    if (
        policy == SourcePolicy.DOMAIN_RESTRICTED &&
        (allowedDomains.isEmpty() || allowedDomains.size > MAX_DOMAINS)
    ) {
        return invalidResearch(
            "research_domains_invalid",
            "domain_restricted requires one to five valid public hosts.",
        )
    }
    val rawSourceUrls = args.stringArray("source_urls")
    val sourceUrls = rawSourceUrls
        ?.mapNotNull { value -> value.toHttpUrlOrNull()?.takeIf(::safePublicResearchUrl) }
        ?.distinctBy { it.toString() }
        .orEmpty()
    if (rawSourceUrls != null && rawSourceUrls.size != sourceUrls.size) {
        return invalidResearch(
            "research_source_urls_invalid",
            "source_urls must contain unique public http or https URLs without credentials.",
        )
    }
    val maxSources = args["max_sources"]?.jsonPrimitive?.intOrNull ?: DEFAULT_MAX_SOURCES
    if (maxSources !in 1..MAX_SOURCES) {
        return invalidResearch("research_max_sources_invalid", "max_sources must be from one to five.")
    }
    return ResearchRequestResult.Ready(
        ResearchRequest(
            query,
            purpose,
            policy,
            allowedDomains,
            sourceUrls,
            maxSources,
        ),
    )
}

private fun invalidResearch(code: String, message: String) = ResearchRequestResult.Failure(
    PublicResearchOutcome(
        code = code,
        state = CapabilityState.READY,
        data = buildJsonObject {
            put("ok", false)
            put("error", message)
            put("read_only", true)
            put("credential_context_kind", "isolated_public_request")
        },
        retryable = false,
    ),
)

private fun normalizeDomain(raw: String): String? {
    val domain = raw.trim().trimEnd('.').lowercase()
    if (
        domain.isBlank() ||
        domain.length > 253 ||
        domain == "localhost" ||
        domain.endsWith(".localhost") ||
        domain.any { !(it.isLetterOrDigit() || it == '-' || it == '.') } ||
        domain.split('.').any { it.isBlank() || it.startsWith('-') || it.endsWith('-') } ||
        !publicNetworkHost(domain)
    ) {
        return null
    }
    return domain
}

internal fun safePublicResearchUrl(url: HttpUrl): Boolean =
    url.scheme in setOf("http", "https") &&
        url.username.isEmpty() &&
        url.password.isEmpty() &&
        publicNetworkHost(url.host) &&
        url.toString().length <= MAX_URL_CHARS

private fun JsonObject.string(name: String): String? =
    get(name)?.jsonPrimitive?.contentOrNull

private fun JsonObject.stringArray(name: String): List<String>? =
    (get(name) as? JsonArray)?.mapNotNull { it.jsonPrimitive.contentOrNull }

private const val MAX_QUERY_CHARS = 512
private const val MAX_PURPOSE_CHARS = 512
private const val MAX_EFFECTIVE_QUERY_CHARS = 1_024
private const val MAX_DOMAINS = 5
private const val MAX_SOURCES = 5
private const val DEFAULT_MAX_SOURCES = 5
private const val MAX_URL_CHARS = 4_096

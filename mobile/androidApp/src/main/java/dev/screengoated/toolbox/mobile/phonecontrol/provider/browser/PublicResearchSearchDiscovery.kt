package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import java.util.Base64
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import org.jsoup.nodes.Document
import org.jsoup.nodes.Element

internal data class SearchResultLink(
    val url: HttpUrl,
    val label: String,
)

/**
 * Capability adapter for search discovery, ported from the Windows
 * `research/search_provider.rs` and `research/search_links.js` boundary.
 * Only links with result structure on the provider's own result page become
 * candidates, so a consent, captcha, sign-in or error page yields no results
 * and discovery falls through to the next provider.
 */
internal class SearchProvider(
    endpoint: String,
    private val transportHost: String,
    private val redirectPath: String,
    private val redirectParameters: Set<String>,
    private val encodedRedirectParameter: String? = null,
) {
    private val endpointUrl = endpoint.toHttpUrl()

    fun searchUrl(query: String): HttpUrl =
        endpointUrl.newBuilder().addQueryParameter("q", query).build()

    /** A response that redirected away from the result endpoint is an interstitial, not results. */
    fun isResultPage(finalUrl: HttpUrl): Boolean =
        finalUrl.host.equals(endpointUrl.host, ignoreCase = true) &&
            finalUrl.encodedPath == endpointUrl.encodedPath

    fun resultLinks(document: Document): List<SearchResultLink> =
        structuralResultLinks(document).mapNotNull { (href, label) ->
            candidate(href)?.let { SearchResultLink(it, label) }
        }.take(MAX_SEARCH_CANDIDATES)

    /**
     * Convert one result-page link into an ordinary external candidate. Links on
     * the exact transport host are either explicitly unwrapped or discarded.
     */
    fun candidate(raw: String): HttpUrl? {
        val parsed = raw.toHttpUrlOrNull() ?: return null
        if (!parsed.host.equals(transportHost, ignoreCase = true)) return parsed
        if (parsed.encodedPath != redirectPath) return null
        redirectParameters.forEach { parameter ->
            parsed.queryParameter(parameter)?.toHttpUrlOrNull()?.let { return it }
        }
        val encoded = encodedRedirectParameter?.let(parsed::queryParameter) ?: return null
        encoded.toHttpUrlOrNull()?.let { return it }
        val decoded = runCatching {
            String(Base64.getUrlDecoder().decode(encoded.removePrefix("a1")), Charsets.UTF_8)
        }.getOrNull()
        return decoded?.toHttpUrlOrNull()
    }
}

internal val SEARCH_PROVIDERS = listOf(
    SearchProvider(
        endpoint = "https://www.google.com/search",
        transportHost = "www.google.com",
        redirectPath = "/url",
        redirectParameters = setOf("q", "url"),
    ),
    SearchProvider(
        endpoint = "https://www.bing.com/search",
        transportHost = "www.bing.com",
        redirectPath = "/ck/a",
        redirectParameters = emptySet(),
        encodedRedirectParameter = "u",
    ),
    SearchProvider(
        endpoint = "https://html.duckduckgo.com/html/",
        transportHost = "duckduckgo.com",
        redirectPath = "/l/",
        redirectParameters = setOf("uddg"),
    ),
)

/**
 * Static-markup port of Windows `search_links.js`: skip hidden anchors and page
 * chrome, then keep labelled http(s) anchors that own or sit in a heading, or
 * sit in an article, list item or main region.
 */
private fun structuralResultLinks(document: Document): List<Pair<String, String>> {
    val links = ArrayList<Pair<String, String>>()
    val anchors = document.select("a[href]")
    for (index in 0 until minOf(anchors.size, MAX_SCANNED_ANCHORS)) {
        if (links.size >= MAX_RESULT_LINKS) break
        val anchor = anchors[index]
        if (anchor.hiddenByMarkup() || anchor.closest(PAGE_CHROME) != null) continue
        if (!anchor.hasResultHeading() && anchor.closest(RESULT_CONTAINER) == null) continue
        val label = listOf(anchor.text(), anchor.attr("aria-label"))
            .map(String::normalizeWhitespace)
            .firstOrNull(String::isNotEmpty)
            ?: continue
        val href = anchor.absUrl("href")
        if (href.length > MAX_HREF_CHARS || !HTTP_HREF.containsMatchIn(href)) continue
        links += href to label.take(MAX_LABEL_CHARS)
    }
    return links
}

private fun Element.hasResultHeading(): Boolean {
    val own = selectFirst(HEADING)
    if (own != null && !own.hiddenByMarkup()) return true
    val containing = closest(HEADING)
    return containing != null && !containing.hiddenByMarkup()
}

private fun Element.hiddenByMarkup(): Boolean =
    (sequenceOf(this) + parents().asSequence()).any { element ->
        element.hasAttr("hidden") ||
            element.attr("aria-hidden").equals("true", ignoreCase = true) ||
            element.normalName() in NON_RENDERED_ELEMENTS ||
            HIDDEN_STYLE.containsMatchIn(element.attr("style"))
    }

internal fun String.normalizeWhitespace(): String = replace(WHITESPACE, " ").trim()

private const val MAX_SCANNED_ANCHORS = 2_000
private const val MAX_RESULT_LINKS = 128
private const val MAX_SEARCH_CANDIDATES = 40
private const val MAX_HREF_CHARS = 2_048
private const val MAX_LABEL_CHARS = 256
private const val HEADING = "h1,h2,h3,h4,[role=heading]"
private const val PAGE_CHROME =
    "header,nav,footer,aside,[role=banner],[role=navigation],[role=contentinfo]"
private const val RESULT_CONTAINER = "article,[role=article],[role=listitem],main,[role=main]"
private val NON_RENDERED_ELEMENTS = setOf("template", "noscript", "script", "style")
private val HTTP_HREF = Regex("^https?://", RegexOption.IGNORE_CASE)
private val HIDDEN_STYLE = Regex(
    "(?:^|;)\\s*(?:display\\s*:\\s*none|visibility\\s*:\\s*(?:hidden|collapse)|opacity\\s*:\\s*0(?:\\.0*)?\\s*(?:;|$))",
    RegexOption.IGNORE_CASE,
)
private val WHITESPACE = Regex("\\s+")

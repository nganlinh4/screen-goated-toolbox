package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import java.net.Proxy
import java.util.concurrent.TimeUnit
import okhttp3.HttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response

internal const val MAX_PUBLIC_RESEARCH_REDIRECTS = 5

/** Follow only destinations approved before their requests are created. */
internal fun executePublicResearchGet(
    client: OkHttpClient,
    request: Request,
    accepts: (HttpUrl) -> Boolean,
): Response? {
    if (request.method != "GET") return null
    // Also enforce this when a caller supplies a client configured to redirect.
    val isolatedClient = client.newBuilder()
        .followRedirects(false)
        .followSslRedirects(false)
        .proxy(Proxy.NO_PROXY)
        .build()
    val budgetMs = client.callTimeoutMillis.takeIf { it > 0 } ?: 15_000
    val started = System.nanoTime()
    val budgetNanos = TimeUnit.MILLISECONDS.toNanos(budgetMs.toLong())
    val visited = mutableSetOf<String>()
    var current = request.url
    for (redirectCount in 0..MAX_PUBLIC_RESEARCH_REDIRECTS) {
        if (!safePublicResearchUrl(current) || !accepts(current)) return null
        val identity = current.newBuilder().fragment(null).build().toString()
        if (!visited.add(identity)) return null
        val remaining = budgetNanos - (System.nanoTime() - started)
        if (remaining <= 0) return null
        val call = isolatedClient.newCall(request.newBuilder().url(current).build())
        call.timeout().timeout(remaining, TimeUnit.NANOSECONDS)
        val response = call.execute()
        if (response.code !in REDIRECT_STATUSES) return response
        val location = response.header("Location")
        response.close()
        if (redirectCount == MAX_PUBLIC_RESEARCH_REDIRECTS || location == null) return null
        current = current.resolve(location) ?: return null
    }
    return null
}

private val REDIRECT_STATUSES = setOf(301, 302, 303, 307, 308)

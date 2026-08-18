package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import okhttp3.Headers

/**
 * Latest rate-limit response observed for each provider endpoint in this app
 * session. Role IDs never participate in identity because text, vision, audio,
 * and search descriptors can point at the same API endpoint.
 */
object ModelUsageStats {
    const val FRESH_THROUGH_SECONDS = 300L
    const val AGING_THROUGH_SECONDS = 900L
    private val localRuntimeProviders = setOf(
        PresetModelProvider.OLLAMA,
        PresetModelProvider.PARAKEET,
        PresetModelProvider.MOONSHINE,
    )

    enum class UsageScope {
        PROVIDER,
        ENDPOINT,
    }

    data class UsageKey(
        val provider: PresetModelProvider,
        val scope: UsageScope,
        val fullName: String? = null,
    )

    enum class MetricKind(val label: String) {
        REQUESTS_DAY("RPD"),
        REQUESTS_MINUTE("RPM"),
        TOKENS_MINUTE("TPM"),
        TOKENS_DAY("TPD"),
        AUDIO_SECONDS_HOUR("ASH"),
        AUDIO_SECONDS_DAY("ASD"),
    }

    data class UsageMetric(
        val kind: MetricKind,
        val remaining: String?,
        val limit: String?,
        val reset: String?,
    )

    data class UsageSnapshot(
        val metrics: List<UsageMetric>,
        val observedAtUnixSeconds: Long,
    )

    enum class Freshness {
        FRESH,
        AGING,
        STALE,
    }

    private val mutableSnapshots = MutableStateFlow<Map<UsageKey, UsageSnapshot>>(emptyMap())
    val snapshots: StateFlow<Map<UsageKey, UsageSnapshot>> = mutableSnapshots.asStateFlow()

    @Synchronized
    fun update(
        provider: PresetModelProvider,
        fullName: String,
        headers: Headers,
        observedAtUnixSeconds: Long = nowUnixSeconds(),
    ) {
        val snapshot = snapshotFromHeaders(provider, headers, observedAtUnixSeconds) ?: return
        mutableSnapshots.value = mutableSnapshots.value + (keyForResponse(provider, fullName) to snapshot)
    }

    fun endpointKey(provider: PresetModelProvider, fullName: String): UsageKey =
        UsageKey(
            provider = provider,
            scope = UsageScope.ENDPOINT,
            fullName = fullName.trim(),
        )

    fun providerKey(provider: PresetModelProvider): UsageKey =
        UsageKey(provider = provider, scope = UsageScope.PROVIDER)

    fun providerHasUsageStatistics(provider: PresetModelProvider): Boolean =
        provider !in localRuntimeProviders

    fun keyForResponse(provider: PresetModelProvider, fullName: String): UsageKey =
        if (provider == PresetModelProvider.OPENROUTER) {
            providerKey(provider)
        } else {
            endpointKey(provider, fullName)
        }

    fun freshnessAt(observedAtUnixSeconds: Long, nowUnixSeconds: Long): Freshness {
        val age = (nowUnixSeconds - observedAtUnixSeconds).coerceAtLeast(0)
        return when {
            age <= FRESH_THROUGH_SECONDS -> Freshness.FRESH
            age <= AGING_THROUGH_SECONDS -> Freshness.AGING
            else -> Freshness.STALE
        }
    }

    fun nowUnixSeconds(): Long = System.currentTimeMillis() / 1_000L

    @Synchronized
    fun clear() {
        mutableSnapshots.value = emptyMap()
    }

    internal fun snapshotFromHeaders(
        provider: PresetModelProvider,
        headers: Headers,
        observedAtUnixSeconds: Long,
    ): UsageSnapshot? {
        val metrics = mutableListOf<UsageMetric>()
        when (provider) {
            PresetModelProvider.OPENROUTER -> {
                metrics.addMetric(
                    headers,
                    MetricKind.REQUESTS_DAY,
                    HeaderTriple(
                        "x-ratelimit-remaining",
                        "x-ratelimit-limit",
                        "x-ratelimit-reset",
                    ),
                )
            }
            else -> metrics.addCommonMetrics(headers)
        }

        metrics.addMetric(
            headers,
            MetricKind.AUDIO_SECONDS_HOUR,
            HeaderTriple(
                "x-ratelimit-remaining-audio-seconds-hour",
                "x-ratelimit-limit-audio-seconds-hour",
                "x-ratelimit-reset-audio-seconds-hour",
            ),
        )
        metrics.addMetric(
            headers,
            MetricKind.AUDIO_SECONDS_DAY,
            HeaderTriple(
                "x-ratelimit-remaining-audio-seconds-day",
                "x-ratelimit-limit-audio-seconds-day",
                "x-ratelimit-reset-audio-seconds-day",
            ),
        )

        return metrics
            .takeIf { it.isNotEmpty() }
            ?.sortedBy { it.kind.ordinal }
            ?.let { UsageSnapshot(it, observedAtUnixSeconds) }
    }

    private data class HeaderTriple(
        val remaining: String,
        val limit: String,
        val reset: String,
    )

    private fun MutableList<UsageMetric>.addCommonMetrics(headers: Headers) {
        addMetric(
            headers,
            MetricKind.REQUESTS_DAY,
            HeaderTriple(
                "x-ratelimit-remaining-requests",
                "x-ratelimit-limit-requests",
                "x-ratelimit-reset-requests",
            ),
        )
        addMetric(
            headers,
            MetricKind.TOKENS_MINUTE,
            HeaderTriple(
                "x-ratelimit-remaining-tokens",
                "x-ratelimit-limit-tokens",
                "x-ratelimit-reset-tokens",
            ),
        )
    }

    private fun MutableList<UsageMetric>.addMetric(
        headers: Headers,
        kind: MetricKind,
        names: HeaderTriple,
    ) {
        val remaining = headers.cleanValue(names.remaining)
        val limit = headers.cleanValue(names.limit)
        if (remaining == null && limit == null) return
        add(
            UsageMetric(
                kind = kind,
                remaining = remaining,
                limit = limit,
                reset = headers.cleanValue(names.reset),
            ),
        )
    }

    private fun Headers.cleanValue(name: String): String? =
        get(name)
            ?.trim()
            ?.takeIf { it.isNotEmpty() }
            ?.take(80)
}

package dev.screengoated.toolbox.mobile.preset

import okhttp3.Headers
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ModelUsageStatsTest {
    @After
    fun resetStore() {
        ModelUsageStats.clear()
    }

    @Test
    fun endpointIdentityCollapsesRolesButKeepsProvidersDistinct() {
        val first = headers(
            "x-ratelimit-remaining-requests" to "19",
            "x-ratelimit-limit-requests" to "20",
        )
        val second = headers(
            "x-ratelimit-remaining-requests" to "18",
            "x-ratelimit-limit-requests" to "20",
        )

        ModelUsageStats.update(PresetModelProvider.GROQ, "vendor/model", first, 100)
        ModelUsageStats.update(PresetModelProvider.GROQ, "vendor/model", second, 101)
        assertEquals(1, ModelUsageStats.snapshots.value.size)
        assertEquals(
            "18",
            ModelUsageStats.snapshots.value
                .getValue(ModelUsageStats.endpointKey(PresetModelProvider.GROQ, "vendor/model"))
                .metrics
                .single()
                .remaining,
        )

        assertNotEquals(
            ModelUsageStats.endpointKey(PresetModelProvider.GROQ, "vendor/model"),
            ModelUsageStats.endpointKey(PresetModelProvider.CEREBRAS, "vendor/model"),
        )
    }

    @Test
    fun cerebrasHeadersRemainIndependentTypedBuckets() {
        val snapshot = ModelUsageStats.snapshotFromHeaders(
            PresetModelProvider.CEREBRAS,
            headers(
                "x-ratelimit-remaining-requests-day" to "14399",
                "x-ratelimit-limit-requests-day" to "14400",
                "x-ratelimit-remaining-tokens-minute" to "59000",
                "x-ratelimit-limit-tokens-minute" to "60000",
            ),
            123,
        )!!

        assertEquals(123, snapshot.observedAtUnixSeconds)
        assertEquals(
            listOf(
                ModelUsageStats.MetricKind.REQUESTS_DAY,
                ModelUsageStats.MetricKind.TOKENS_MINUTE,
            ),
            snapshot.metrics.map { it.kind },
        )
    }

    @Test
    fun openRouterUsesOneProviderScopeAndIgnoresEmptyHeaders() {
        assertEquals(
            ModelUsageStats.keyForResponse(PresetModelProvider.OPENROUTER, "first/model"),
            ModelUsageStats.keyForResponse(PresetModelProvider.OPENROUTER, "second/model"),
        )
        assertNull(
            ModelUsageStats.snapshotFromHeaders(
                PresetModelProvider.OPENROUTER,
                Headers.Builder().build(),
                123,
            ),
        )
    }

    @Test
    fun openRouterProviderHeadersBecomeOneDailyRequestBucket() {
        val snapshot = ModelUsageStats.snapshotFromHeaders(
            PresetModelProvider.OPENROUTER,
            headers(
                "x-ratelimit-remaining" to "49",
                "x-ratelimit-limit" to "50",
                "x-ratelimit-reset" to "86400",
            ),
            123,
        )!!
        assertEquals(1, snapshot.metrics.size)
        assertEquals(ModelUsageStats.MetricKind.REQUESTS_DAY, snapshot.metrics.single().kind)
        assertEquals("49", snapshot.metrics.single().remaining)
        assertEquals("50", snapshot.metrics.single().limit)
    }

    @Test
    fun freshnessThresholdsMatchSharedContract() {
        assertEquals(
            ModelUsageStats.Freshness.FRESH,
            ModelUsageStats.freshnessAt(1_000, 1_300),
        )
        assertEquals(
            ModelUsageStats.Freshness.AGING,
            ModelUsageStats.freshnessAt(1_000, 1_301),
        )
        assertEquals(
            ModelUsageStats.Freshness.STALE,
            ModelUsageStats.freshnessAt(1_000, 1_901),
        )
    }

    private fun headers(vararg values: Pair<String, String>): Headers =
        Headers.Builder()
            .apply { values.forEach { (name, value) -> add(name, value) } }
            .build()
}

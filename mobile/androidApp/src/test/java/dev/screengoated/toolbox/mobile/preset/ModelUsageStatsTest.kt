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
            ModelUsageStats.endpointKey(PresetModelProvider.OPENROUTER, "vendor/model"),
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

    @Test
    fun tokenBudgetBlocksOnlyCertainShortfallsAndProjectsContinuousRefill() {
        val provider = PresetModelProvider.GROQ
        val model = "vendor/vision-model"
        ModelUsageStats.update(
            provider,
            model,
            headers(
                "x-ratelimit-remaining-tokens" to "0",
                "x-ratelimit-limit-tokens" to "8000",
                "x-ratelimit-reset-tokens" to "20s",
            ),
            observedAtUnixSeconds = 100,
        )

        assertEquals(4L, ModelUsageStats.tokenBudgetWaitSeconds(provider, model, 1_282, 100))
        assertNull(ModelUsageStats.tokenBudgetWaitSeconds(provider, model, 1_282, 110))
    }

    @Test
    fun unknownOrMalformedTokenWindowNeverBlocks() {
        assertNull(
            ModelUsageStats.tokenBudgetWaitSeconds(
                PresetModelProvider.GROQ,
                "never-observed",
                5_000,
                100,
            ),
        )
        ModelUsageStats.update(
            PresetModelProvider.GROQ,
            "missing-reset",
            headers(
                "x-ratelimit-remaining-tokens" to "0",
                "x-ratelimit-limit-tokens" to "8000",
            ),
            observedAtUnixSeconds = 100,
        )
        assertNull(
            ModelUsageStats.tokenBudgetWaitSeconds(
                PresetModelProvider.GROQ,
                "missing-reset",
                5_000,
                100,
            ),
        )
    }

    private fun headers(vararg values: Pair<String, String>): Headers =
        Headers.Builder()
            .apply { values.forEach { (name, value) -> add(name, value) } }
            .build()
}

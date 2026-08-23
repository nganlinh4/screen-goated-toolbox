package dev.screengoated.toolbox.mobile.preset

import java.io.File
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetModelFeedTest {
    @After
    fun clearFeed() {
        PresetModelFeed.publish(null)
    }

    @Test
    fun repositoryPublishedFeedVerifiesWithRawCoordinatePublicKey() {
        val root = repoRoot()
        val payload = File(root, "tests/fixtures/model-feed/published-feed.json").readBytes()
        val signature = File(root, "tests/fixtures/model-feed/published-feed.json.sig").readBytes()
        val publicKey = decodeFeedPublicKey(
            File(root, "monitoring/monitoring-p256-public-key.hex").readText().trim(),
        )

        val feed = parseVerifiedAvailabilityFeed(publicKey, payload, signature)
        assertEquals("nvidia", feed.provider)
        assertEquals(1, feed.schemaVersion)

        val tampered = payload.clone().also { it[it.lastIndex] = (it.last().toInt() xor 1).toByte() }
        assertTrue(runCatching { parseVerifiedAvailabilityFeed(publicKey, tampered, signature) }.isFailure)
    }

    @Test
    fun operationalGateRanksUsefulModelsWithoutPresetSpecificQualityBans() {
        val feed = schemaThreeFeed(
            model("nvidia/slower", 900, 1.0, 6),
            model("nvidia/fast", 180, 0.83, 6),
            model("nvidia/flaky", 100, 0.79, 20),
            model("nvidia/unmeasured", 10, 1.0, 0),
        )

        assertEquals(
            listOf("nvidia/fast", "nvidia/slower"),
            rankedFeedModels(feed).map(FeedModel::endpoint),
        )
    }

    @Test
    fun dedicatedTranslationEndpointNeverEntersGenericPriorityChains() {
        assertEquals(
            PresetModelType.TEXT,
            feedModelType(model("nvidia/general-8b", 100, 1.0, 3)),
        )
        assertEquals(
            null,
            feedModelType(model("nvidia/riva-translate-4b-instruct-v2", 100, 1.0, 3)),
        )
    }

    @Test
    fun discoveredEndpointsReceiveStableIdsAndCompactProviderNames() {
        val first = discoveredModelId("nvidia", "nvidia/new-fast-9b-instruct")
        val second = discoveredModelId("nvidia", "nvidia/new-fast-10b-instruct")

        assertEquals(first, discoveredModelId("nvidia", "nvidia/new-fast-9b-instruct"))
        assertNotEquals(first, second)
        assertEquals("N nf9i", compactEndpointName("nvidia", "nvidia/new-fast-9b-instruct"))
    }

    @Test
    fun adaptiveRowsInterleaveBelowHeadAndManualEditsStayLiveWhileOneRemains() {
        val liveFast = "nvidia-nemotron-3-5-lightning-text"
        val liveNext = "nvidia-nemotron-3-super-120b-text"
        PresetModelFeed.publish(
            schemaThreeFeed(
                model("nvidia/nemotron-3.5-lightning-30b-a3b", 100, 1.0, 6),
                model("nvidia/nemotron-3-super-120b-a12b", 180, 1.0, 6),
            ),
        )
        val configured = listOf(
            "groq-qwen-3-6-27b-text",
            "google-gemini-3-5-flash-lite-text",
            "google-gemini-3-flash-text",
        )
        val settings = PresetRuntimeSettings(
            modelPriorityChains = PresetModelPriorityChains(textToText = configured),
        )
        val merged = PresetRetryChainKind.TEXT_TO_TEXT.effectiveChain(
            settings,
            ApiKeys(nvidiaKey = "test-key"),
        )

        assertEquals(configured.first(), merged.first())
        assertTrue(merged.indexOf(liveFast) in 1 until merged.size)
        assertTrue(merged.indexOf(liveNext) in 1 until merged.size)

        val oneRemoved = commitAdaptiveEdits(
            visible = merged.filterNot { it == liveNext },
            currentOverrides = PresetLiveModelOverrides(),
            liveIds = listOf(liveFast, liveNext),
            edits = listOf(AdaptiveManualEdit.Remove(liveNext)),
        )
        assertTrue(oneRemoved.remainsEnabled)
        assertTrue(liveNext in oneRemoved.overrides.excluded)

        val allRemoved = commitAdaptiveEdits(
            visible = oneRemoved.authored.filterNot { it == liveFast },
            currentOverrides = oneRemoved.overrides,
            liveIds = listOf(liveFast, liveNext),
            edits = listOf(AdaptiveManualEdit.Remove(liveFast)),
        )
        assertFalse(allRemoved.remainsEnabled)
    }

    private fun schemaThreeFeed(vararg models: FeedModel) = AvailabilityFeed(
        schemaVersion = 3,
        controlVersion = 1,
        availabilityGateVersion = 1,
        provider = "nvidia",
        generatedAt = "2026-08-23T00:00:00Z",
        models = models.toList(),
    )

    private fun model(
        endpoint: String,
        p50Ms: Int,
        successRate: Double,
        runs: Int,
    ) = FeedModel(endpoint, FeedReasoningControl.PLAIN, "text", p50Ms, successRate, runs)

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root -> File(root, "tests/fixtures/model-feed/published-feed.json").exists() }
            ?: error("Could not locate model-feed fixtures from $workingDirectory")
    }
}

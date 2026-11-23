package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.inputAdapter
import dev.screengoated.toolbox.mobile.shared.preset.textBlock
import java.io.IOException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * A provider attempt that claimed the half-open probe and is then cancelled while suspended
 * in the request must give the probe back; otherwise the model stays "probe in flight" for
 * the process lifetime.
 */
class PresetProbeCancellationTest {
    private val model = PresetModelCatalog.forType(PresetModelType.TEXT)
        .first { it.provider == PresetModelProvider.GOOGLE }
    private val keys = ApiKeys(geminiKey = "test-key")
    private val entered = CountDownLatch(1)
    private val blockingClient = OkHttpClient.Builder().addInterceptor { chain ->
        entered.countDown()
        val giveUpAt = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        while (!chain.call().isCanceled() && System.nanoTime() < giveUpAt) Thread.sleep(5)
        throw IOException("Canceled")
    }.build()

    @Before
    fun openExpiredCircuit() {
        clearPresetModelCircuitsForTest()
        val hourAgo = System.nanoTime() / 1_000_000L - TimeUnit.HOURS.toMillis(1)
        recordPresetModelFailureAt(model.id, "HTTP 429 too many requests", hourAgo)
    }

    @After
    fun clearCircuits() = clearPresetModelCircuitsForTest()

    @Test
    fun cancelledGraphAttempt_releasesItsHalfOpenProbe() = runBlocking {
        val sessionId = "probe-cancel-graph"
        val state = MutableStateFlow(PresetExecutionState(sessionId = sessionId, isExecuting = true))
        val executor = PresetGraphExecutor(
            textApiClient = TextApiClient(blockingClient),
            visionApiClient = VisionApiClient(blockingClient),
            apiKeys = { keys },
            runtimeSettings = { PresetRuntimeSettings() },
            uiLanguage = { "en" },
            executionState = state,
            toastBus = AppToastBus(),
        )
        val preset = Preset(
            id = "probe-cancel", nameEn = "p", nameVi = "p", nameKo = "p",
            presetType = PresetType.TEXT_INPUT,
            blocks = listOf(inputAdapter(), textBlock(model.id, "Echo").copy(showOverlay = false)),
            blockConnections = listOf(0 to 1),
        )

        val job = launch(Dispatchers.Default) {
            executor.executeGraph(sessionId, preset, PresetInput.Text("hello"))
        }
        assertProbeHeldWhileInFlight()
        job.cancelAndJoin()

        assertNull(presetModelCircuitSkipReason(model.id))
    }

    @Test
    fun cancelledRefineAttempt_releasesItsHalfOpenProbe() = runBlocking {
        val main = Executors.newSingleThreadExecutor()
        try {
            val repository = PresetRepository(
                textApiClient = TextApiClient(blockingClient),
                visionApiClient = VisionApiClient(blockingClient),
                apiKeys = { keys },
                runtimeSettings = { PresetRuntimeSettings() },
                uiLanguage = { "en" },
                overrideStore = MemoryStore(),
                toastBus = AppToastBus(),
                mainDispatcher = main.asCoroutineDispatcher(),
            )
            val job = repository.refineInPlace(
                originalModelId = model.id,
                previousText = "previous",
                refinePrompt = "shorter",
                onChunk = {},
                onModelSelected = {},
                onComplete = {},
            )
            assertProbeHeldWhileInFlight()
            job.cancelAndJoin()

            assertNull(presetModelCircuitSkipReason(model.id))
        } finally {
            main.shutdownNow()
        }
    }

    private fun assertProbeHeldWhileInFlight() {
        assertTrue("provider request never started", entered.await(10, TimeUnit.SECONDS))
        assertEquals("MODEL_COOLDOWN_PROBE_IN_FLIGHT:${model.id}", presetModelCircuitSkipReason(model.id))
    }

    private class MemoryStore : PresetOverrideStore {
        override fun load() = StoredPresetOverrides()
        override fun save(overrides: StoredPresetOverrides) = Unit
    }
}

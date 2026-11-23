package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock
import dev.screengoated.toolbox.mobile.shared.preset.inputAdapter
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PresetPostProcessActionsOwnershipTest {
    @Test
    fun reassignedPostProcessActions_areUsedByNextExecution() = runTest {
        val repo = repository(StandardTestDispatcher(testScheduler))
        val first = CountingActions()
        val second = CountingActions()

        repo.postProcessActions = first
        repo.executePreset(copyPreset("first"), PresetInput.Text("hello"))
        runCurrent()
        assertEquals(1, first.copies)

        // A recreated overlay controller installs new actions; the next run must use them.
        repo.postProcessActions = second
        repo.executePreset(copyPreset("second"), PresetInput.Text("hello"))
        runCurrent()
        assertEquals(1, first.copies)
        assertEquals(1, second.copies)
    }

    @Test
    fun releasingOnlyClearsTheCurrentOwner() {
        val repo = repository(StandardTestDispatcher())
        val stale = CountingActions()
        val current = CountingActions()
        repo.postProcessActions = current

        repo.releasePostProcessActions(stale)
        assertSame(current, repo.postProcessActions)
        repo.releasePostProcessActions(current)
        assertSame(NoOpPostProcessActions, repo.postProcessActions)
    }

    private fun repository(dispatcher: TestDispatcher): PresetRepository {
        val client = OkHttpClient()
        return PresetRepository(
            textApiClient = TextApiClient(client), visionApiClient = VisionApiClient(client),
            apiKeys = { ApiKeys() }, runtimeSettings = { PresetRuntimeSettings() }, uiLanguage = { "en" },
            overrideStore = MemoryStore(), toastBus = AppToastBus(), mainDispatcher = dispatcher,
        )
    }

    private fun copyPreset(id: String) = Preset(
        id = id, nameEn = id, nameVi = id, nameKo = id, presetType = PresetType.TEXT_INPUT,
        blocks = listOf(inputAdapter().copy(showOverlay = true, autoCopy = true)),
    )

    private class MemoryStore : PresetOverrideStore {
        override fun load() = StoredPresetOverrides()
        override fun save(overrides: StoredPresetOverrides) = Unit
    }

    private class CountingActions : PresetPostProcessActions {
        var copies = 0
        override fun handleAutoCopy(block: ProcessingBlock, resultText: String, lease: PresetEffectLease) { copies++ }
        override fun handleAutoCopyImage(block: ProcessingBlock, pngBytes: ByteArray, lease: PresetEffectLease) = Unit
        override fun handleAutoSpeak(block: ProcessingBlock, resultText: String, blockIdx: Int, lease: PresetEffectLease) = Unit
        override fun handleAutoPaste(lease: PresetEffectLease) = Unit
    }
}

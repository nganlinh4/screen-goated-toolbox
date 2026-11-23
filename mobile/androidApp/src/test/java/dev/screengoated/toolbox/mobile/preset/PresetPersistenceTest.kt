package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.WindowGeometry
import dev.screengoated.toolbox.mobile.preset.ui.attemptPresetEdit
import dev.screengoated.toolbox.mobile.preset.ui.PresetTextDraft
import dev.screengoated.toolbox.mobile.preset.ui.savePresetTextDraft
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Assume.assumeNoException
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.IOException
import java.nio.file.Files
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

@OptIn(ExperimentalCoroutinesApi::class)
class PresetPersistenceTest {
    @get:Rule val temporary = TemporaryFolder()
    private val json = Json { ignoreUnknownKeys = true }
    private val saved = StoredPresetOverrides(
        builtInOverrides = mapOf("preset_translate" to PresetOverride(isFavorite = true)),
    )

    @Test
    fun duplicateSavedIdsUseExactStorageRowsAndLegacyIdCallsFailClosed() {
        val builtin = dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets.all.first()
        val first = builtin.copy(nameEn = "custom first")
        val second = builtin.copy(nameEn = "custom second")
        val store = object : PresetOverrideStore {
            var value = StoredPresetOverrides(customPresets = linkedMapOf("legacy-a" to first, "legacy-b" to second))
            override fun load() = value
            override fun save(overrides: StoredPresetOverrides) { value = overrides }
        }
        val repository = repository(store) { "fresh-custom" }
        fun identity(key: String) = repository.catalogState.value.presets.single { it.rowKey == key }.rowIdentity
        val firstRow = identity(customPresetRowKey("legacy-a"))
        val secondRow = identity(customPresetRowKey("legacy-b"))
        val builtInRow = identity(builtInPresetRowKey(builtin.id))
        assertEquals(null, repository.getResolvedPreset(builtin.id))
        repository.deletePreset(builtin.id)
        assertFalse(repository.updateBuiltInOverride(builtin.id) { it.copy(nameEn = "wrong") })
        assertEquals(first, store.value.customPresets["legacy-a"])
        assertEquals(second, store.value.customPresets["legacy-b"])

        assertTrue(repository.updatePresetRow(secondRow) { it.copy(nameEn = "updated second") })
        assertEquals(first, store.value.customPresets["legacy-a"])
        assertEquals("updated second", store.value.customPresets["legacy-b"]?.nameEn)
        assertEquals("fresh-custom", repository.duplicatePresetRow(secondRow, "en"))
        assertEquals("updated second Copy", store.value.customPresets["fresh-custom"]?.nameEn)
        assertTrue(repository.deletePresetRow(firstRow))
        assertFalse(store.value.customPresets.containsKey("legacy-a"))
        assertEquals("updated second", store.value.customPresets["legacy-b"]?.nameEn)
        assertEquals(builtin, repository.catalogState.value.findRow(builtInRow)?.preset)
    }

    @Test
    fun removedAndRestoredRowRejectsActionsFromItsPreviousIncarnation() {
        val store = object : PresetOverrideStore {
            var value = StoredPresetOverrides()
            override fun load() = value
            override fun save(overrides: StoredPresetOverrides) { value = overrides }
        }
        val repository = repository(store)
        val id = "preset_translate"
        val original = requireNotNull(repository.catalogState.value.findPreset(id)).rowIdentity
        assertTrue(repository.deletePresetRow(original))
        repository.restoreBuiltInPreset(id)
        val restored = requireNotNull(repository.catalogState.value.findPreset(id)).rowIdentity
        assertNotEquals(original, restored)
        assertTrue(repository.updatePresetRow(restored) { it.copy(isFavorite = true) })
        assertFalse(repository.restorePresetRow(original))
        assertEquals(true, store.value.builtInOverrides[id]?.isFavorite)
        assertFalse(repository.updatePresetRow(original) { it.copy(nameEn = "stale") })
        assertFalse(repository.deletePresetRow(original))
        assertTrue(repository.catalogState.value.findRow(restored) != null)
    }

    @Test
    fun hiddenBuiltinStillMakesDuplicateLegacyIdAmbiguous() {
        val builtin = dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets.all.first()
        val custom = builtin.copy(nameEn = "custom")
        val store = object : PresetOverrideStore {
            var value = StoredPresetOverrides(customPresets = mapOf("legacy" to custom))
            override fun load() = value
            override fun save(overrides: StoredPresetOverrides) { value = overrides }
        }
        val repository = repository(store)
        val builtInRow = repository.catalogState.value.presets.single { it.isBuiltIn && it.preset.id == builtin.id }
        assertTrue(repository.deletePresetRow(builtInRow.rowIdentity))
        assertEquals(null, repository.getResolvedPreset(builtin.id))
        repository.deletePreset(builtin.id)
        assertFalse(repository.updateBuiltInOverride(builtin.id) { it.copy(nameEn = "wrong") })
        assertEquals(custom, store.value.customPresets["legacy"])
    }

    @Test
    fun failedDeletePreservesRowAndItsSelectionIdentity() {
        val store = object : PresetOverrideStore {
            override fun load() = StoredPresetOverrides()
            override fun save(overrides: StoredPresetOverrides) { throw IOException("injected") }
        }
        val repository = repository(store)
        val row = requireNotNull(repository.catalogState.value.findPreset("preset_translate"))
        assertFalse(repository.deletePresetRow(row.rowIdentity))
        assertEquals(row.rowIdentity, repository.catalogState.value.findPreset("preset_translate")?.rowIdentity)
        assertEquals(row.preset, repository.catalogState.value.findRow(row.rowIdentity)?.preset)
    }

    @Test
    fun recreatedRepositoryRejectsSavedIdentityFromAnOlderProcess() {
        val store = object : PresetOverrideStore {
            var value = StoredPresetOverrides()
            override fun load() = value
            override fun save(overrides: StoredPresetOverrides) { value = overrides }
        }
        val oldRepository = repository(store)
        val old = requireNotNull(oldRepository.catalogState.value.findPreset("preset_translate")).rowIdentity
        val newRepository = repository(store)
        val current = requireNotNull(newRepository.catalogState.value.findPreset("preset_translate")).rowIdentity
        assertNotEquals(old, current)
        assertFalse(newRepository.deletePresetRow(old))
        assertFalse(newRepository.restorePresetRow(old))
        assertTrue(newRepository.catalogState.value.findRow(current) != null)
    }

    @Test
    fun missingFileCanBeSavedAndReopened() {
        val file = temporary.root.resolve("presets.json")
        val store = PresetPersistence(file, json)
        assertEquals(StoredPresetOverrides(), store.load())
        store.save(saved)
        assertEquals(saved, PresetPersistence(file, json).load())
        assertEquals(listOf("presets.json"), temporary.root.listFiles()!!.map { it.name })
    }

    @Test
    fun corruptUnknownAndUnsupportedDocumentsCannotBeOverwritten() {
        val file = temporary.root.resolve("presets.json")
        val fixture = generateSequence(temporary.root.absoluteFile) { it.parentFile }
            .map { it.resolve("parity-fixtures/preset-system/persistence.json") }
            .firstOrNull { it.isFile }
            ?: generateSequence(java.io.File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) { it.parentFile }
                .map { it.resolve("parity-fixtures/preset-system/persistence.json") }
                .first { it.isFile }
        val cases = json.parseToJsonElement(fixture.readText()).jsonObject.getValue("cases").jsonArray
        val invalid = cases.first { it.jsonObject.getValue("name").jsonPrimitive.contentOrNull == "invalid_document" }.jsonObject
        for (sample in invalid.getValue("samples").jsonArray) {
            val contents = sample.jsonPrimitive.content
            file.writeText(contents)
            val store = PresetPersistence(file, json)
            assertFails { store.load() }
            assertFails { store.save(saved) }
            assertEquals(contents, file.readText())
        }
    }

    @Test
    fun unreadableExistingPathDoesNotBecomeAnEmptyDocument() {
        val directory = temporary.newFolder("preset-storage")
        val store = PresetPersistence(directory, json)
        assertFails { store.load() }
        assertFails { store.save(saved) }
        assertTrue(directory.isDirectory)
    }

    @Test
    fun invalidUtf8IsPreservedInsteadOfBeingReplacedDuringDecode() {
        val file = temporary.root.resolve("presets.json")
        val bytes = byteArrayOf(0x7b, 0x22, 0x78, 0x22, 0x3a, 0x22, 0xc3.toByte(), 0x22, 0x7d)
        file.writeBytes(bytes)
        val store = PresetPersistence(file, json)
        assertFails { store.load() }
        assertFails { store.save(saved) }
        assertTrue(bytes.contentEquals(file.readBytes()))
    }

    @Test
    fun failedReplacementPreservesTheOriginalAndRemovesOnlyItsTemporaryFile() {
        val file = temporary.root.resolve("presets.json")
        val initial = PresetPersistence(file, json)
        initial.load()
        initial.save(saved)
        val original = file.readBytes()
        val failing = PresetPersistence(file, json) { _, _ -> throw IOException("replace failed") }
        assertEquals(saved, failing.load())
        assertFails { failing.save(StoredPresetOverrides()) }
        assertTrue(original.contentEquals(file.readBytes()))
        assertEquals(listOf("presets.json"), temporary.root.listFiles()!!.map { it.name })
    }

    @Test
    fun staleStoreCannotOverwriteChangesPublishedByAnotherStore() {
        val file = temporary.root.resolve("presets.json")
        val first = PresetPersistence(file, json)
        val second = PresetPersistence(file, json)
        first.load()
        second.load()
        first.save(saved)
        assertFails { second.save(StoredPresetOverrides()) }
        assertEquals(saved, PresetPersistence(file, json).load())
    }

    @Test
    fun symbolicLinkIsNeverReadOrReplaced() {
        val target = temporary.newFile("target.json")
        target.writeText("{}")
        val link = temporary.root.resolve("presets.json").toPath()
        try {
            Files.createSymbolicLink(link, target.toPath())
        } catch (error: Exception) {
            assumeNoException("Symbolic links unavailable on this test host", error)
        }
        val store = PresetPersistence(link.toFile(), json)
        assertFails { store.load() }
        assertFails { store.save(saved) }
        assertTrue(Files.isSymbolicLink(link))
        assertEquals("{}", target.readText())
    }

    @Test
    fun staleEditorChangesPreserveNewerOverlayBounds() {
        val store = object : PresetOverrideStore {
            var value = StoredPresetOverrides()
            override fun load() = value
            override fun save(overrides: StoredPresetOverrides) { value = overrides }
        }
        val repository = repository(store)
        val before = requireNotNull(repository.getResolvedPreset("preset_translate")).preset
        val bounds = WindowGeometry(x = 10, y = 20, width = 300, height = 200)
        repository.updateBuiltInOverride(before.id) { it.copy(windowGeometry = bounds) }
        val updated = before.copy(isFavorite = !before.isFavorite)
        repository.updateBuiltInOverride(before.id) { current -> mergePresetEdit(current, before, updated) }
        val result = requireNotNull(repository.getResolvedPreset(before.id)).preset
        assertEquals(bounds, result.windowGeometry)
        assertEquals(updated.isFavorite, result.isFavorite)
    }

    @Test
    fun lateOverlayUpdateCannotUnhideDeletedBuiltin() {
        val store = object : PresetOverrideStore {
            var value = StoredPresetOverrides()
            override fun load() = value
            override fun save(overrides: StoredPresetOverrides) { value = overrides }
        }
        val repository = repository(store)
        val id = "preset_translate"
        repository.deletePreset(id)
        repository.updateBuiltInOverride(id) { it.copy(windowGeometry = WindowGeometry(10, 20, 300, 200)) }
        assertEquals(null, repository.getResolvedPreset(id))
        assertEquals(true, store.value.builtInOverrides[id]?.isHidden)
    }

    @Test
    fun generatedIdentityCollisionRetriesWithoutReplacingExistingPreset() {
        val existing = dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets.all.first()
            .copy(id = "existing")
        val store = object : PresetOverrideStore {
            var value = StoredPresetOverrides(customPresets = mapOf(existing.id to existing))
            override fun load() = value
            override fun save(overrides: StoredPresetOverrides) { value = overrides }
        }
        val ids = listOf("existing", "preset_unknown", "new-id").iterator()
        val repository = repository(store) { ids.next() }
        assertEquals("new-id", repository.createCustomPreset(PresetType.TEXT_INPUT, "en"))
        assertEquals(existing, store.value.customPresets["existing"])
        assertTrue(store.value.customPresets.containsKey("new-id"))
        val exhausted = repository(store) { "existing" }
        assertEquals(null, exhausted.createCustomPreset(PresetType.TEXT_INPUT, "en"))
        assertEquals(existing, store.value.customPresets["existing"])
    }

    @Test
    fun repositoryDoesNotPublishAnEditWhenStorageFails() {
        val store = object : PresetOverrideStore {
            override fun load() = StoredPresetOverrides()
            override fun save(overrides: StoredPresetOverrides) { throw IOException("disk full") }
        }
        val repository = repository(store)
        val id = "preset_translate"
        val previous = requireNotNull(repository.getResolvedPreset(id)).preset.isFavorite
        repository.toggleFavorite(id)
        assertEquals(previous, requireNotNull(repository.getResolvedPreset(id)).preset.isFavorite)
        assertEquals(PresetPersistenceFailure.WRITE, repository.catalogState.value.persistenceFailure)
    }

    @Test
    fun failedEditorSaveRevertsStateAndCanBeRetried() {
        val store = object : PresetOverrideStore {
            var value = StoredPresetOverrides()
            var failNext = true
            override fun load() = value
            override fun save(overrides: StoredPresetOverrides) {
                if (failNext) {
                    failNext = false
                    throw IOException("disk full")
                }
                value = overrides
            }
        }
        val repository = repository(store)
        val initial = requireNotNull(repository.getResolvedPreset("preset_translate")).preset
        val edited = initial.copy(isFavorite = !initial.isFavorite)
        val save: (dev.screengoated.toolbox.mobile.shared.preset.Preset, dev.screengoated.toolbox.mobile.shared.preset.Preset) -> Boolean =
            { old, next -> repository.updateBuiltInOverride(old.id) { mergePresetEdit(it, old, next) } }
        val failed = attemptPresetEdit(initial, edited, save)
        assertFalse(failed.saved)
        assertEquals(initial, failed.state)
        val retried = attemptPresetEdit(failed.state, edited, save)
        assertTrue(retried.saved)
        assertEquals(edited, retried.state)
        val nextEdit = retried.state.copy(showControllerUi = !retried.state.showControllerUi)
        assertTrue(attemptPresetEdit(retried.state, nextEdit, save).saved)
        val current = requireNotNull(repository.getResolvedPreset(initial.id)).preset
        assertEquals(edited.isFavorite, current.isFavorite)
        assertEquals(nextEdit.showControllerUi, current.showControllerUi)
    }

    @Test
    fun failedPromptAndNameDraftsStayOpenUntilRetrySucceeds() {
        for (prompt in listOf(false, true)) {
            val store = object : PresetOverrideStore {
                var value = StoredPresetOverrides()
                var failNext = true
                override fun load() = value
                override fun save(overrides: StoredPresetOverrides) {
                    if (failNext) {
                        failNext = false
                        throw IOException("disk full")
                    }
                    value = overrides
                }
            }
            val repository = repository(store)
            var editor = requireNotNull(repository.getResolvedPreset("preset_translate")).preset
            val draft = PresetTextDraft("new value", true)
            val save: (String) -> Boolean = { text ->
                val updated = if (prompt) {
                    editor.copy(blocks = editor.blocks.mapIndexed { i, block ->
                        if (i == 0) block.copy(prompt = text) else block
                    })
                } else editor.copy(nameEn = text)
                val attempt = attemptPresetEdit(editor, updated) { old, next ->
                    repository.updateBuiltInOverride(old.id) { mergePresetEdit(it, old, next) }
                }
                editor = attempt.state
                attempt.saved
            }
            val failed = savePresetTextDraft(draft, save)
            assertTrue(failed.isOpen)
            assertEquals("new value", failed.text)
            val retried = savePresetTextDraft(failed, save)
            assertFalse(retried.isOpen)
            val saved = requireNotNull(repository.getResolvedPreset(editor.id)).preset
            assertEquals("new value", if (prompt) saved.blocks.first().prompt else saved.nameEn)
        }
    }

    @Test
    fun repositoryBlocksAllEditsAfterFailedLoad() {
        var writes = 0
        val store = object : PresetOverrideStore {
            override fun load(): StoredPresetOverrides = throw IOException("unreadable")
            override fun save(overrides: StoredPresetOverrides) { writes++ }
        }
        val repository = repository(store)
        val id = "preset_translate"
        val previous = requireNotNull(repository.getResolvedPreset(id)).preset.isFavorite
        repository.toggleFavorite(id)
        assertEquals(0, writes)
        assertEquals(previous, requireNotNull(repository.getResolvedPreset(id)).preset.isFavorite)
        assertEquals(PresetPersistenceFailure.READ, repository.catalogState.value.persistenceFailure)
        assertFalse(repository.catalogState.value.presets.isEmpty())
    }

    @Test
    fun concurrentRepositoryEditsPreserveBothUpdates() {
        val firstSaveEntered = CountDownLatch(1)
        val releaseFirstSave = CountDownLatch(1)
        val secondStarted = CountDownLatch(1)
        val count = AtomicInteger()
        val store = object : PresetOverrideStore {
            var persisted = StoredPresetOverrides()
            override fun load() = persisted
            override fun save(overrides: StoredPresetOverrides) {
                if (count.getAndIncrement() == 0) {
                    firstSaveEntered.countDown()
                    check(releaseFirstSave.await(5, TimeUnit.SECONDS))
                }
                persisted = overrides
            }
        }
        val repository = repository(store)
        val pool = Executors.newFixedThreadPool(2)
        try {
            val first = pool.submit { repository.toggleFavorite("preset_translate") }
            assertTrue(firstSaveEntered.await(5, TimeUnit.SECONDS))
            val second = pool.submit {
                secondStarted.countDown()
                repository.toggleFavorite("preset_ocr")
            }
            assertTrue(secondStarted.await(5, TimeUnit.SECONDS))
            releaseFirstSave.countDown()
            first.get(5, TimeUnit.SECONDS)
            second.get(5, TimeUnit.SECONDS)
            assertEquals(true, store.persisted.builtInOverrides["preset_translate"]?.isFavorite)
            assertEquals(true, store.persisted.builtInOverrides["preset_ocr"]?.isFavorite)
        } finally {
            releaseFirstSave.countDown()
            pool.shutdownNow()
        }
    }

    private fun repository(store: PresetOverrideStore, newId: () -> String = { java.util.UUID.randomUUID().toString() }) = PresetRepository(
        textApiClient = TextApiClient(OkHttpClient()),
        visionApiClient = VisionApiClient(OkHttpClient()),
        apiKeys = { ApiKeys() },
        runtimeSettings = { PresetRuntimeSettings() },
        uiLanguage = { "en" },
        overrideStore = store,
        toastBus = AppToastBus(),
        mainDispatcher = StandardTestDispatcher(),
        newPresetId = newId,
    )

    private fun assertFails(action: () -> Unit) {
        try {
            action()
            fail("Expected persistence failure")
        } catch (_: Exception) {
            // Expected: callers must not mistake failed reads or writes for success.
        }
    }
}

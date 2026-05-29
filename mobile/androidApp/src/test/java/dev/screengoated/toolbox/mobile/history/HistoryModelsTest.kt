package dev.screengoated.toolbox.mobile.history

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

@OptIn(ExperimentalCoroutinesApi::class)
class HistoryModelsTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun constantsMatchSharedHistoryFixture() {
        val root = json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString()).jsonObject
        val range = root.getValue("maxItemsRange").jsonObject

        assertEquals(root.getValue("defaultMaxItems").jsonPrimitive.int, DEFAULT_HISTORY_LIMIT)
        assertEquals(range.getValue("min").jsonPrimitive.int, MIN_HISTORY_LIMIT)
        assertEquals(range.getValue("max").jsonPrimitive.int, MAX_HISTORY_LIMIT)
    }

    @Test
    fun clampHistoryLimitUsesWindowsBounds() {
        assertEquals(MIN_HISTORY_LIMIT, clampHistoryLimit(MIN_HISTORY_LIMIT - 9))
        assertEquals(DEFAULT_HISTORY_LIMIT, clampHistoryLimit(DEFAULT_HISTORY_LIMIT))
        assertEquals(MAX_HISTORY_LIMIT, clampHistoryLimit(MAX_HISTORY_LIMIT + 25))
    }

    @Test
    fun normalizeHistorySettingsMigratesLegacyImplicit200Default() {
        val case = fixtureCase("legacy_android_default_200_migrates_to_windows_default_50")
        val normalized = normalizeHistorySettings(
            HistorySettings(
                maxItems = case.getValue("storedSettings").jsonObject.getValue("maxItems").jsonPrimitive.int,
                hasExplicitMaxItems = false,
            ),
        )

        assertEquals(case.getValue("expectedNormalizedMaxItems").jsonPrimitive.int, normalized.maxItems)
    }

    @Test
    fun normalizeHistorySettingsKeepsExplicit200Selection() {
        val normalized = normalizeHistorySettings(
            HistorySettings(
                maxItems = MAX_HISTORY_LIMIT,
                hasExplicitMaxItems = true,
            ),
        )

        assertEquals(MAX_HISTORY_LIMIT, normalized.maxItems)
    }

    @Test
    fun filterHistoryItemsMatchesTextAndTimestampOnly() {
        val items = listOf(
            HistoryItem(
                id = 1L,
                timestamp = "2026-03-21 10:15:00",
                itemType = HistoryType.TEXT,
                text = "Translated hello world",
                mediaPath = "hello.txt",
            ),
            HistoryItem(
                id = 2L,
                timestamp = "2026-03-22 08:00:00",
                itemType = HistoryType.IMAGE,
                text = "Receipt summary",
                mediaPath = "invoice.png",
            ),
        )

        assertEquals(listOf(items.first()), filterHistoryItems(items, "hello"))
        assertEquals(listOf(items.last()), filterHistoryItems(items, "2026-03-22"))
        assertEquals(emptyList<HistoryItem>(), filterHistoryItems(items, "invoice"))
    }

    @Test
    fun filterHistoryItemsMatchesFixtureTimestampOnlyCase() {
        val case = fixtureCase("search_matches_text_and_timestamp_only")
        val items = case.getValue("items").jsonArray.map { item ->
            val obj = item.jsonObject
            HistoryItem(
                id = obj.getValue("id").jsonPrimitive.content.toLong(),
                timestamp = obj.getValue("timestamp").jsonPrimitive.content,
                itemType = HistoryType.valueOf(obj.getValue("itemType").jsonPrimitive.content),
                text = obj.getValue("text").jsonPrimitive.content,
                mediaPath = obj.getValue("mediaPath").jsonPrimitive.content,
            )
        }

        assertEquals(
            case.getValue("expectedMatchCount").jsonPrimitive.int,
            filterHistoryItems(items, case.getValue("query").jsonPrimitive.content).size,
        )
        assertEquals(emptyList<HistoryItem>(), filterHistoryItems(items, "text_1.txt"))
    }

    @Test
    fun repositorySavesTextNewestFirstAndKeepsSourceBackingFile() = runTest {
        val dispatcher = StandardTestDispatcher(testScheduler)
        val repository = createRepository(dispatcher)
        val orderCase = fixtureCase("save_text_newest_first")
        val backingCase = fixtureCase("text_item_keeps_source_in_backing_file")

        orderCase.getValue("operations").jsonArray.forEach { operation ->
            val obj = operation.jsonObject
            repository.saveText(
                resultText = obj.getValue("resultText").jsonPrimitive.content,
                inputText = obj.getValue("sourceText").jsonPrimitive.content,
            )
        }
        advanceUntilIdle()

        assertEquals(
            orderCase.getValue("expectedOrder").jsonArray.map { it.jsonPrimitive.content },
            repository.state.value.items.map { it.text },
        )

        val backingOperation = backingCase.getValue("operations").jsonArray.single().jsonObject
        val backingRepository = createRepository(dispatcher)
        backingRepository.saveText(
            resultText = backingOperation.getValue("resultText").jsonPrimitive.content,
            inputText = backingOperation.getValue("sourceText").jsonPrimitive.content,
        )
        advanceUntilIdle()
        val item = backingRepository.state.value.items.single()

        assertEquals(
            HistoryType.valueOf(backingCase.getValue("expectedItemType").jsonPrimitive.content),
            item.itemType,
        )
        assertEquals(backingCase.getValue("expectedVisibleText").jsonPrimitive.content, item.text)
        assertEquals(
            backingCase.getValue("expectedBackingText").jsonPrimitive.content,
            requireNotNull(backingRepository.mediaFileFor(item)).readText(),
        )
    }

    @Test
    fun rapidSavesKeepUniqueBackingFilesFromFixture() = runTest {
        val dispatcher = StandardTestDispatcher(testScheduler)
        val repository = createRepository(dispatcher)
        val case = fixtureCase("rapid_saves_keep_unique_backing_files")

        case.getValue("operations").jsonArray.forEach { operation ->
            val obj = operation.jsonObject
            repository.saveText(
                resultText = obj.getValue("resultText").jsonPrimitive.content,
                inputText = obj.getValue("sourceText").jsonPrimitive.content,
            )
        }
        advanceUntilIdle()

        val items = repository.state.value.items
        if (case.getValue("expectedUniqueMediaPaths").jsonPrimitive.content.toBoolean()) {
            assertEquals(items.size, items.map { it.mediaPath }.toSet().size)
        }

        val expectedByResult = case.getValue("expectedBackingTextsByResult").jsonObject
        expectedByResult.forEach { (resultText, expectedBackingText) ->
            val item = requireNotNull(items.firstOrNull { it.text == resultText })
            assertTrue(requireNotNull(repository.mediaFileFor(item)).exists())
            assertEquals(
                expectedBackingText.jsonPrimitive.content,
                requireNotNull(repository.mediaFileFor(item)).readText(),
            )
        }
    }

    @Test
    fun repositoryPrunesDeletesClearsAndResetsFromFixture() = runTest {
        val dispatcher = StandardTestDispatcher(testScheduler)
        val pruneRepository = createRepository(dispatcher)
        val pruneCase = fixtureCase("prune_removes_oldest")

        pruneCase.getValue("operations").jsonArray.forEach { operation ->
            val obj = operation.jsonObject
            when (obj.getValue("type").jsonPrimitive.content) {
                "save_text" -> pruneRepository.saveText(
                    resultText = obj.getValue("resultText").jsonPrimitive.content,
                    inputText = obj.getValue("sourceText").jsonPrimitive.content,
                )
                "set_max_items" -> pruneRepository.updateMaxItems(
                    obj.getValue("value").jsonPrimitive.int,
                )
                else -> error("Unsupported history fixture operation: $obj")
            }
        }
        advanceUntilIdle()

        assertEquals(
            pruneCase.getValue("expectedOrder").jsonArray.map { it.jsonPrimitive.content },
            pruneRepository.state.value.items.map { it.text },
        )

        val resetRepository = createRepository(dispatcher)
        resetRepository.updateMaxItems(MAX_HISTORY_LIMIT)
        advanceUntilIdle()
        resetRepository.resetSettingsToDefaults()
        advanceUntilIdle()
        assertEquals(
            fixtureCase("settings_reset_restores_history_default_50")
                .getValue("expectedNormalizedMaxItems")
                .jsonPrimitive
                .int,
            resetRepository.state.value.maxItems,
        )

        val deleteRepository = createRepository(dispatcher)
        val deleteCase = fixtureCase("delete_and_clear_all_remove_items")
        deleteCase.getValue("operations").jsonArray.forEach { operation ->
            val obj = operation.jsonObject
            when (obj.getValue("type").jsonPrimitive.content) {
                "save_text" -> deleteRepository.saveText(
                    resultText = obj.getValue("resultText").jsonPrimitive.content,
                    inputText = obj.getValue("sourceText").jsonPrimitive.content,
                )
                "delete_by_result_text" -> {
                    advanceUntilIdle()
                    val id = requireNotNull(
                        deleteRepository.state.value.items.firstOrNull {
                            it.text == obj.getValue("resultText").jsonPrimitive.content
                        },
                    ).id
                    deleteRepository.delete(id)
                }
                "clear_all" -> deleteRepository.clearAll()
                else -> error("Unsupported history fixture operation: $obj")
            }
        }
        advanceUntilIdle()

        assertEquals(
            deleteCase.getValue("expectedCount").jsonPrimitive.int,
            deleteRepository.state.value.items.size,
        )
    }

    @Test
    fun clearAllRemovesOrphanMediaFilesFromFixture() = runTest {
        val dispatcher = StandardTestDispatcher(testScheduler)
        val repository = createRepository(dispatcher)
        val case = fixtureCase("clear_all_removes_orphan_media_files")
        val mediaDir = repository.mediaDirectory()

        case.getValue("orphanMediaFiles").jsonArray.forEach { fileName ->
            val file = mediaDir.resolve(fileName.jsonPrimitive.content)
            file.parentFile?.mkdirs()
            file.writeText("orphan")
        }

        repository.clearAll()
        advanceUntilIdle()

        assertEquals(
            case.getValue("expectedRemainingMediaFiles").jsonArray.map { it.jsonPrimitive.content },
            mediaDir.listFiles()?.map { it.name }?.sorted().orEmpty(),
        )
    }

    private fun createRepository(
        dispatcher: CoroutineDispatcher,
    ): HistoryRepository {
        val root = Files.createTempDirectory("sgt-history-test").toFile()
        return HistoryRepository(
            persistence = HistoryPersistence(
                paths = HistoryPaths(
                    rootDir = root,
                    databaseFile = File(root, "history.json"),
                    settingsFile = File(root, "history_settings.json"),
                    mediaDir = File(root, "history_media"),
                    supportsFolderOpen = true,
                ),
                json = json,
            ),
            ioDispatcher = dispatcher,
        )
    }

    private fun fixtureCase(name: String) = json
        .parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString())
        .jsonObject
        .getValue("cases")
        .jsonArray
        .map { it.jsonObject }
        .firstOrNull { it.getValue("name").jsonPrimitive.content == name }
        ?: error("Missing fixture case: $name")

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "history-ui", "state-machine.json"),
            Paths.get("..", "..", "parity-fixtures", "history-ui", "state-machine.json"),
            Paths.get("parity-fixtures", "history-ui", "state-machine.json"),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing history fixture. Tried: $candidates")
    }
}

package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.model.CustomModelState
import dev.screengoated.toolbox.mobile.model.CustomModelStatus
import dev.screengoated.toolbox.mobile.storage.CustomModelDataUnavailableException
import java.nio.file.Files
import java.nio.file.Paths
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CustomModelSnapshotTest {
    @Test fun sharedOwnershipCasesUseStablePublishedSnapshots() {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("preset-system/custom-model-ownership.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        val previous = PresetCustomModelRegistry.definitions()
        val first = CustomPresetModelDefinition(id = "first", displayName = "First", fullName = "first")
        val second = CustomPresetModelDefinition(id = "second", displayName = "Second", fullName = "second")
        val input = mutableListOf(first)
        try {
            PresetCustomModelRegistry.set(input)
            val old = PresetCustomModelRegistry.definitions()
            for (element in contract.getValue("cases").jsonArray) {
                val case = element.jsonObject
                val actual = when (case.getValue("event").jsonPrimitive.content) {
                    "mutate_input_after_publish" -> {
                        input.clear()
                        if (PresetCustomModelRegistry.definitions() == listOf(first)) "published_snapshot_unchanged" else "changed"
                    }
                    "mutate_returned_snapshot" -> {
                        assertThrows(UnsupportedOperationException::class.java) { (old as MutableList).clear() }
                        "rejected"
                    }
                    "publish_replacement" -> {
                        PresetCustomModelRegistry.set(listOf(second))
                        if (PresetCustomModelRegistry.definitions() == listOf(second)) "new_snapshot_visible" else "changed"
                    }
                    "mutate_old_snapshot_after_replacement" -> {
                        assertThrows(UnsupportedOperationException::class.java) { (old as MutableList).clear() }
                        if (PresetCustomModelRegistry.definitions() == listOf(second)) "current_snapshot_unchanged" else "changed"
                    }
                    else -> error("Unknown snapshot case")
                }
                assertEquals(case.getValue("expected").jsonPrimitive.content, actual)
            }
        } finally { PresetCustomModelRegistry.set(previous) }
    }

    @Test fun concurrentPublicationsDoNotLoseRevisionUpdates() {
        val previous = PresetCustomModelRegistry.definitions()
        val before = PresetCustomModelRegistry.revision.value
        val threads = 4
        val count = 200
        val start = CountDownLatch(1)
        val pool = Executors.newFixedThreadPool(threads)
        try {
            val tasks = (0 until threads).map { index ->
                pool.submit {
                    check(start.await(5, TimeUnit.SECONDS))
                    repeat(count) {
                        PresetCustomModelRegistry.set(listOf(CustomPresetModelDefinition(
                            id = "$index-$it", displayName = "Model", fullName = "model",
                        )))
                    }
                }
            }
            start.countDown()
            tasks.forEach { it.get(10, TimeUnit.SECONDS) }
            assertEquals(before + threads * count, PresetCustomModelRegistry.revision.value)
        } finally {
            start.countDown()
            pool.shutdownNow()
            PresetCustomModelRegistry.set(previous)
        }
    }

    @Test fun customModelStatePublishesOwnedSnapshotsAndFencesUnreadableData() = runTest {
        val previous = PresetCustomModelRegistry.definitions()
        val first = CustomPresetModelDefinition(id = "first", displayName = "First", fullName = "first")
        val second = CustomPresetModelDefinition(id = "second", displayName = "Second", fullName = "second")
        val input = mutableListOf(first)
        var saved: List<CustomPresetModelDefinition>? = null
        try {
            val state = CustomModelState({ input }, { saved = it }, StandardTestDispatcher(testScheduler))
            state.publish()
            input.clear()
            assertEquals(listOf(first), state.models.value)
            val replacement = mutableListOf(second)
            state.update(replacement)
            replacement.clear()
            advanceUntilIdle()
            assertEquals(listOf(second), state.models.value)
            assertEquals(listOf(second), PresetCustomModelRegistry.definitions())
            assertEquals(listOf(second), saved)
            val unavailable = CustomModelState(
                { throw CustomModelDataUnavailableException(IllegalArgumentException("invalid")) },
                { error("Unreadable data must not be overwritten") },
            )
            unavailable.publish()
            assertEquals(emptyList<CustomPresetModelDefinition>(), PresetCustomModelRegistry.definitions())
            assertFalse(unavailable.update(listOf(first)))
            assertFalse(unavailable.retry())
        } finally { PresetCustomModelRegistry.set(previous) }
    }

    @Test fun failedSaveDoesNotPublishTheCandidate() = runTest {
        val previous = PresetCustomModelRegistry.definitions()
        val first = CustomPresetModelDefinition(id = "first", displayName = "First", fullName = "first")
        val second = CustomPresetModelDefinition(id = "second", displayName = "Second", fullName = "second")
        try {
            val state = CustomModelState({ listOf(first) }, { error("Synthetic storage failure") }, StandardTestDispatcher(testScheduler))
            state.publish()
            assertTrue(state.update(listOf(second)))
            advanceUntilIdle()
            assertEquals(listOf(second), state.models.value)
            assertEquals(listOf(first), PresetCustomModelRegistry.definitions())
            assertEquals(CustomModelStatus.WRITE_UNCERTAIN, state.status.value)
        } finally { PresetCustomModelRegistry.set(previous) }
    }

    @Test fun sharedWriteFailuresFenceLaterUpdatesWithoutClaimingRollback() = runTest {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("preset-system/custom-model-write-failures.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        val previous = PresetCustomModelRegistry.definitions()
        val old = CustomPresetModelDefinition(id = "old", displayName = "Old", fullName = "old")
        val fresh = CustomPresetModelDefinition(id = "new", displayName = "New", fullName = "new")
        try {
            for (element in contract.getValue("cases").jsonArray) {
                val case = element.jsonObject
                val event = case.getValue("event").jsonPrimitive.content
                var stored = listOf(old)
                var attempts = 0
                val submitted = mutableListOf<List<CustomPresetModelDefinition>>()
                val state = CustomModelState({ stored }, { candidate ->
                    attempts++
                    submitted += candidate
                    if (attempts == 1) assertEquals(listOf(old), PresetCustomModelRegistry.definitions())
                    if (event != "failure_before_effect") stored = candidate
                    if (event != "accepted" && (event != "retry_uncertain" || attempts == 1)) error("Synthetic uncertain save")
                }, StandardTestDispatcher(testScheduler))
                state.publish()
                val attempted = mutableListOf(fresh)
                val firstAttempt = state.update(attempted)
                attempted.clear()
                advanceUntilIdle()
                val accepted = if (event == "retry_uncertain") {
                    assertTrue(firstAttempt)
                    assertEquals(CustomModelStatus.WRITE_UNCERTAIN, state.status.value)
                    assertTrue(state.retry())
                    advanceUntilIdle()
                    assertEquals(listOf(listOf(fresh), listOf(fresh)), submitted)
                    true
                } else state.status.value == CustomModelStatus.AVAILABLE
                val expectedPublished = case.getValue("published").jsonPrimitive.content
                assertEquals(listOf(fresh), state.models.value)
                assertEquals(if (expectedPublished == "new") listOf(fresh) else listOf(old), PresetCustomModelRegistry.definitions())
                assertEquals(event == "accepted" || event == "retry_uncertain", accepted)
                if (event == "failure_after_effect") assertEquals(listOf(fresh), stored)
                val later = state.update(listOf(old))
                assertEquals(case.getValue("later_writes").jsonPrimitive.content == "allowed", later)
                advanceUntilIdle()
                if (event != "accepted" && event != "retry_uncertain") {
                    assertEquals(1, attempts)
                    assertEquals(CustomModelStatus.WRITE_UNCERTAIN, state.status.value)
                } else {
                    assertEquals(if (event == "accepted") 2 else 3, attempts)
                    assertTrue(state.available)
                }
            }
        } finally { PresetCustomModelRegistry.set(previous) }
    }

    @Test fun anUncertainWriteFencesConcurrentEdits() {
        val previous = PresetCustomModelRegistry.definitions()
        val old = CustomPresetModelDefinition(id = "old", displayName = "Old", fullName = "old")
        val fresh = CustomPresetModelDefinition(id = "new", displayName = "New", fullName = "new")
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val pool = Executors.newFixedThreadPool(2)
        var attempts = 0
        try {
            val state = CustomModelState({ listOf(old) }, {
                attempts++
                entered.countDown()
                check(release.await(5, TimeUnit.SECONDS))
                error("Synthetic uncertain save")
            })
            state.publish()
            val first = pool.submit<Boolean> { state.update(listOf(fresh)) }
            assertTrue(entered.await(5, TimeUnit.SECONDS))
            val second = pool.submit<Boolean> { state.update(emptyList()) }
            assertTrue(first.get(5, TimeUnit.SECONDS))
            assertTrue(second.get(5, TimeUnit.SECONDS))
            release.countDown()
            runBlocking { withTimeout(5_000) { state.status.first { it == CustomModelStatus.WRITE_UNCERTAIN } } }
            assertEquals(1, attempts)
            assertEquals(emptyList<CustomPresetModelDefinition>(), state.models.value)
            assertEquals(listOf(old), PresetCustomModelRegistry.definitions())
        } finally {
            release.countDown()
            pool.shutdownNow()
            PresetCustomModelRegistry.set(previous)
        }
    }
}

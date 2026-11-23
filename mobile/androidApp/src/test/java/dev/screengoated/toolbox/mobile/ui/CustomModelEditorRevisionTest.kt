package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.model.CustomModelState
import dev.screengoated.toolbox.mobile.model.CustomModelStatus
import dev.screengoated.toolbox.mobile.model.CustomModelSubmission
import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.preset.PresetCustomModelRegistry
import java.nio.file.Files
import java.nio.file.Paths
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.Before
import org.junit.After

@OptIn(ExperimentalCoroutinesApi::class)
class CustomModelEditorRevisionTest {
    private lateinit var previous: List<CustomPresetModelDefinition>
    @Before fun saveRegistry() { previous = PresetCustomModelRegistry.definitions() }
    @After fun restoreRegistry() { PresetCustomModelRegistry.set(previous) }

    @Test fun failureRetryAndReopenRetainTheLatestSnapshot() = runTest {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("preset-system/custom-model-editor-observations.json") }
            .first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        val events = contract.getValue("cases").jsonArray.map { it.jsonObject.getValue("event").jsonPrimitive.content }.toSet()
        assertEquals(setOf("late_observation_after_edit", "rapid_edits_then_old_observation", "newer_external_edit",
            "failed_write_then_retry", "reopen_with_pending_edit", "edit_based_on_stale_models",
            "save_status_changes_without_model_change"), events)
        val latest = CustomPresetModelDefinition(id = "latest", displayName = "latest", fullName = "latest")
        var attempts = 0
        val gate = CompletableDeferred<Unit>()
        val state = CustomModelState({ emptyList() }, {
            if (++attempts == 1) error("Uncertain write")
            gate.await()
        }, StandardTestDispatcher(testScheduler))
        val accepted = requireNotNull(state.submit(listOf(latest)))
        advanceUntilIdle()
        val failed = state.editor.value
        assertEquals(CustomModelStatus.WRITE_UNCERTAIN, failed.status)
        assertEquals(listOf(latest), failed.models)
        assertTrue(failed.revision > accepted.revision)
        assertTrue(state.retry())
        runCurrent()
        val reopened = state.editor.value
        assertEquals(CustomModelStatus.SAVING, reopened.status)
        assertEquals(listOf(latest), reopened.models)
        assertTrue(reopened.revision > failed.revision)
        gate.complete(Unit)
        advanceUntilIdle()
        assertEquals(CustomModelStatus.AVAILABLE, state.editor.value.status)
    }

    @Test fun staleDraftCannotOverwriteNewerModelsButSaveStatusDoesNotRejectCurrentDraft() = runTest {
        fun model(id: String) = CustomPresetModelDefinition(id = id, displayName = id, fullName = id)
        val initial = listOf(model("initial"))
        val external = listOf(model("external"))
        val edited = listOf(model("edited"))
        val state = CustomModelState({ initial }, {}, StandardTestDispatcher(testScheduler))
        requireNotNull(state.submit(external))
        val conflict = state.submitIfCurrent(initial, edited) as CustomModelSubmission.Conflict
        assertEquals(external, conflict.snapshot.models)
        assertEquals(external, state.editor.value.models)
        advanceUntilIdle()
        assertEquals(CustomModelStatus.AVAILABLE, state.editor.value.status)
        val accepted = state.submitIfCurrent(external, edited) as CustomModelSubmission.Accepted
        assertEquals(edited, accepted.snapshot.models)
        advanceUntilIdle()
        assertEquals(edited, state.editor.value.models)
    }

    @Test fun lateObservationsCannotReplaceAcceptedDraftAndNewerOnesAreAdmitted() = runTest {
        fun model(id: String) = CustomPresetModelDefinition(id = id, displayName = id, fullName = id)
        val gate = CompletableDeferred<Unit>()
        val state = CustomModelState({ listOf(model("old")) }, { gate.await() }, StandardTestDispatcher(testScheduler))
        val initial = state.editor.value
        val revision = CustomModelEditorRevision(initial.revision)
        val first = requireNotNull(state.submit(listOf(model("first"))))
        runCurrent()
        val latest = requireNotNull(state.submit(listOf(model("latest"))))
        assertTrue(revision.admit(first.revision))
        assertTrue(revision.admit(latest.revision))
        assertFalse(revision.admit(initial.revision))
        assertFalse(revision.admit(first.revision))
        assertEquals("latest", state.editor.value.models.single().id)
        assertEquals(CustomModelStatus.SAVING, state.editor.value.status)
        gate.complete(Unit)
        advanceUntilIdle()
        assertEquals(CustomModelStatus.AVAILABLE, state.editor.value.status)
        assertTrue(revision.admit(state.editor.value.revision))
        val external = state.submit(listOf(model("external")))
        assertNotNull(external)
        assertTrue(revision.admit(requireNotNull(external).revision))
        assertEquals("external", state.editor.value.models.single().id)
        advanceUntilIdle()
    }
}

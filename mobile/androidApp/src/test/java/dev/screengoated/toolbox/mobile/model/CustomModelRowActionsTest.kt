package dev.screengoated.toolbox.mobile.model

import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.preset.PresetCustomModelRegistry
import dev.screengoated.toolbox.mobile.ui.CustomModelActionLifetime
import java.nio.file.Files
import java.nio.file.Paths
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CustomModelRowActionsTest {
    private fun model(id: String) = CustomPresetModelDefinition(id = id, displayName = id, fullName = id)

    @Test fun sharedRowActionCasesReplayAgainstOwner() = runTest {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("preset-system/custom-model-row-actions.json") }.first(Files::exists)
        val cases = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject.getValue("cases").jsonArray
        val previous = PresetCustomModelRegistry.definitions()
        try {
            for (entry in cases) {
                val case = entry.jsonObject
                val event = case.getValue("event").jsonPrimitive.content
                val a = model("a")
                val b = model("b")
                val state = CustomModelState({ listOf(a, b) }, {}, StandardTestDispatcher(testScheduler))
                val aId = state.editor.value.rows[0].identity
                val bId = state.editor.value.rows[1].identity
                val outcome = when (event) {
                    "row_removed_before_delayed_action" -> {
                        state.deleteRow(aId)
                        check(state.editRow(aId) { it.copy(displayName = "stale") } is CustomModelSubmission.Conflict)
                        check(state.models.value == listOf(b)); "target_preserved"
                    }
                    "row_replaced_before_delayed_action" -> {
                        state.submit(listOf(a.copy(displayName = "external"), b))
                        check(state.deleteRow(aId) is CustomModelSubmission.Conflict)
                        check(state.models.value.first().displayName == "external"); "replacement_preserved"
                    }
                    "other_field_changed_before_delayed_edit" -> {
                        state.editRow(bId) { it.copy(displayName = "new name") }
                        state.editRow(bId) { it.copy(fullName = "new api name") }
                        check(state.models.value[1].displayName == "new name"); "newer_field_preserved"
                    }
                    "duplicate_id_before_action" -> {
                        state.submit(listOf(a, a))
                        check(state.deleteRow(state.editor.value.rows.first().identity) is CustomModelSubmission.Conflict)
                        "ambiguous_action_rejected"
                    }
                    "delete_and_recreate_same_id" -> {
                        state.deleteRow(aId); state.submit(listOf(a, b))
                        check(state.deleteRow(aId) is CustomModelSubmission.Conflict)
                        check(state.models.value == listOf(a, b)); "replacement_preserved"
                    }
                    "row_shift_before_action" -> {
                        state.deleteRow(aId)
                        state.editRow(bId) { it.copy(displayName = "edited") }
                        check(state.models.value.single().displayName == "edited"); "original_target_updated"
                    }
                    "reopen_after_row_replacement" -> {
                        val replacement = a.copy(displayName = "replacement")
                        state.submit(listOf(replacement, b))
                        val reopened = CustomModelState({ state.models.value }, {}, StandardTestDispatcher(testScheduler))
                        check(reopened.deleteRow(aId) is CustomModelSubmission.Conflict); "old_action_rejected"
                    }
                    "dialog_closed_before_callback" -> {
                        val dialog = CustomModelActionLifetime()
                        dialog.retire()
                        check(!dialog.run { state.deleteRow(aId) is CustomModelSubmission.Accepted })
                        check(state.models.value == listOf(a, b)); "old_action_rejected"
                    }
                    "editor_retired_before_retry" -> {
                        val oldEditor = CustomModelActionLifetime()
                        oldEditor.retire()
                        val newEditor = CustomModelActionLifetime()
                        check(newEditor.run { true })
                        check(!oldEditor.run { state.deleteRow(aId) is CustomModelSubmission.Accepted })
                        check(state.models.value == listOf(a, b)); "old_action_rejected"
                    }
                    else -> error("Unknown row-action fixture case")
                }
                assertEquals(event, case.getValue("result").jsonPrimitive.content, outcome)
                advanceUntilIdle()
            }
        } finally { PresetCustomModelRegistry.set(previous) }
    }

    @Test fun delayedActionsFollowRowIdentityAndMergeFields() = runTest {
        val previous = PresetCustomModelRegistry.definitions()
        try {
            val a = model("a")
            val b = model("b")
            val state = CustomModelState({ listOf(a, b) }, {}, StandardTestDispatcher(testScheduler))
            val aId = state.editor.value.rows[0].identity
            val bId = state.editor.value.rows[1].identity
            assertTrue(state.deleteRow(aId) is CustomModelSubmission.Accepted)
            assertTrue(state.editRow(bId) { it.copy(displayName = "new name") } is CustomModelSubmission.Accepted)
            assertTrue(state.editRow(bId) { it.copy(fullName = "new api name") } is CustomModelSubmission.Accepted)
            assertEquals("new name", state.editor.value.models.single().displayName)
            assertEquals("new api name", state.editor.value.models.single().fullName)
            assertTrue(state.editRow(aId) { it.copy(displayName = "stale") } is CustomModelSubmission.Conflict)
            requireNotNull(state.submit(state.models.value + a))
            assertTrue(state.deleteRow(aId) is CustomModelSubmission.Conflict)
            assertEquals(listOf("b", "a"), state.models.value.map { it.id })
            advanceUntilIdle()
        } finally { PresetCustomModelRegistry.set(previous) }
    }

    @Test fun duplicateIdsAndChangedExternalRowsRejectOldActions() = runTest {
        val previous = PresetCustomModelRegistry.definitions()
        try {
            val a = model("a")
            val state = CustomModelState({ listOf(a) }, {}, StandardTestDispatcher(testScheduler))
            val original = state.editor.value.rows.single().identity
            requireNotNull(state.submit(listOf(a, a)))
            val duplicate = state.editor.value.rows.first().identity
            assertTrue(state.deleteRow(duplicate) is CustomModelSubmission.Conflict)
            requireNotNull(state.submit(listOf(a.copy(displayName = "external"))))
            assertTrue(state.editRow(original) { it.copy(displayName = "stale") } is CustomModelSubmission.Conflict)
            assertEquals("external", state.models.value.single().displayName)
            advanceUntilIdle()
        } finally { PresetCustomModelRegistry.set(previous) }
    }
}

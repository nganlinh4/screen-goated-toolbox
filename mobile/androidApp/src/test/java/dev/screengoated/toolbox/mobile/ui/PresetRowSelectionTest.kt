package dev.screengoated.toolbox.mobile.ui

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class PresetRowSelectionTest {
    @Test fun sharedDeletionFixturePreservesTheStructuralRow() {
        val fixture = listOf(
            File("../../parity-fixtures/preset-system/list-identity.json"),
            File("../parity-fixtures/preset-system/list-identity.json"),
            File("parity-fixtures/preset-system/list-identity.json"),
        ).first(File::isFile)
        val cases = Json.parseToJsonElement(fixture.readText()).jsonObject.getValue("cases").jsonArray
        for (case in cases.map { it.jsonObject }) {
            val rows = (0 until case.getValue("length").jsonPrimitive.int).map { "row-$it" }
            val selected = case.getValue("selected").jsonPrimitive.content.toIntOrNull()
            val deleted = case.getValue("deleted").jsonPrimitive.int
            val expected = case.getValue("expected_selected").jsonPrimitive.content.toIntOrNull()
            val remaining = rows.filterIndexed { index, _ -> index != deleted }
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                expected?.let { remaining[it] },
                reconcilePresetRowSelection(rows, remaining, selected?.let { rows[it] }),
            )
        }
    }

    @Test fun duplicatePresetIdsDoNotConfuseDistinctStorageRows() {
        val builtin = "built-in:duplicate-id"
        val firstCustom = "custom:legacy-one"
        val secondCustom = "custom:legacy-two"
        val previous = listOf(builtin, firstCustom, secondCustom)
        assertEquals(firstCustom, reconcilePresetRowSelection(previous, previous - builtin, firstCustom))
        assertEquals(builtin, reconcilePresetRowSelection(previous, previous - firstCustom, firstCustom))
        assertEquals(null, reconcilePresetRowSelection(previous, emptyList(), firstCustom))
    }

    @Test fun restoredStorageKeyDoesNotReviveADeletedSelection() {
        val previous = listOf("1:built-in:first", "2:custom:same-key", "3:custom:last")
        val current = listOf("1:built-in:first", "4:custom:same-key", "3:custom:last")
        assertEquals("1:built-in:first", reconcilePresetRowSelection(previous, current, "2:custom:same-key"))
    }

    @Test fun staleEditorCloseCannotClearAReplacement() {
        assertEquals("new", closePresetRowSelection("new", "old"))
        assertEquals(null, closePresetRowSelection("new", "new"))
    }
}

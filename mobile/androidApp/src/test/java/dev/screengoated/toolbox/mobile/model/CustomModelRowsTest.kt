package dev.screengoated.toolbox.mobile.model

import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class CustomModelRowsTest {
    private fun model(id: String, name: String = id) =
        CustomPresetModelDefinition(id = id, displayName = name, fullName = id)

    @Test fun rowShiftRetainsIdentityButDeleteAndRecreateDoesNot() {
        val a = model("a")
        val b = model("b")
        val owner = CustomModelRows(listOf(a, b))
        val original = owner.rows.map { it.identity }
        owner.reconcile(listOf(b))
        assertEquals(original[1], owner.rows.single().identity)
        owner.reconcile(listOf(a, b))
        assertNotEquals(original[0], owner.rows[0].identity)
        assertEquals(original[1], owner.rows[1].identity)
        assertNotEquals(owner.rows[0].identity, CustomModelRows(listOf(a)).rows[0].identity)
    }

    @Test fun onlyAnExplicitRowEditCanRetainIdentityAcrossAChangedValue() {
        val a = model("a")
        val owner = CustomModelRows(listOf(a))
        val first = owner.rows.single().identity
        owner.reconcile(listOf(model("a", "changed")), changedRow = first)
        assertEquals(first, owner.rows.single().identity)
        owner.reconcile(listOf(model("a", "external")))
        assertNotEquals(first, owner.rows.single().identity)
    }

    @Test fun duplicateIdsNeverInheritAnAmbiguousIdentity() {
        val a = model("a")
        val owner = CustomModelRows(listOf(a))
        val first = owner.rows.single().identity
        owner.reconcile(listOf(a, a))
        val duplicates = owner.rows.map { it.identity }
        assertNotEquals(first, duplicates[0])
        assertNotEquals(duplicates[0], duplicates[1])
        owner.reconcile(listOf(a))
        assertNotEquals(duplicates[0], owner.rows.single().identity)
        assertNotEquals(duplicates[1], owner.rows.single().identity)
    }
}

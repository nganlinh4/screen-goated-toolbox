package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AccessibilityTraversalBudgetTest {
    @Test
    fun everyVisitedNodeCountsNotOnlyPublishedOnes() {
        val budget = AccessibilityTraversalBudget(maxVisitedNodes = 3, maxDepth = 10)
        assertTrue(budget.admit(0))
        assertTrue(budget.admit(1))
        assertTrue(budget.admit(1))
        assertFalse(budget.admit(2))
        assertEquals(3, budget.visited)
    }

    @Test
    fun deepTreesStopAtTheDepthBound() {
        val budget = AccessibilityTraversalBudget(maxVisitedNodes = 100, maxDepth = 2)
        assertTrue(budget.admit(2))
        assertFalse(budget.admit(3))
        assertEquals(1, budget.visited)
    }

    @Test
    fun slowTreeStopsAtTheWallClockBound() {
        var nowNanos = 0L
        val budget = AccessibilityTraversalBudget(
            maxVisitedNodes = 100,
            maxDepth = 10,
            maxDurationMs = 5,
            nanoTime = { nowNanos },
        )
        assertTrue(budget.admit(0))
        nowNanos = 4_999_999L
        assertTrue(budget.admit(1))
        nowNanos = 5_000_000L
        assertFalse(budget.admit(1))
        assertEquals(2, budget.visited)
    }

    @Test
    fun defaultTimeBoundStaysWellUnderAnr() {
        assertTrue(ACCESSIBILITY_MAX_TRAVERSAL_MS in 1L..2_000L)
    }

    @Test
    fun defaultsBoundAHugeTree() {
        val budget = AccessibilityTraversalBudget()
        var admitted = 0
        repeat(20_000) { if (budget.admit(1)) admitted += 1 }
        assertEquals(ACCESSIBILITY_MAX_VISITED_NODES, admitted)
        assertFalse(AccessibilityTraversalBudget().admit(ACCESSIBILITY_MAX_TREE_DEPTH + 1))
    }

    @Test
    fun oneBudgetIsSharedByEveryWindowAndCheckedBeforeAnyNodeRead() {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        val traversal = File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/" +
                "provider/accessibility/AccessibilityTraversal.kt",
        ).readText()
        val capture = traversal.substringAfter("internal fun captureAccessibilitySurface(")
            .substringBefore("private fun traverseAccessibilityTree(")
        assertEquals(1, Regex("""AccessibilityTraversalBudget\(\)""").findAll(capture).count())
        assertTrue(
            capture.indexOf("val budget = AccessibilityTraversalBudget()") <
                capture.indexOf("for (window in windows)"),
        )
        val walk = traversal.substringAfter("private fun traverseAccessibilityTree(")
        assertTrue(walk.indexOf("budget.admit(path.size)") < walk.indexOf("node.packageName"))
    }
}

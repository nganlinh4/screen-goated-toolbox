package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

/**
 * Bounds one capture's walk of foreign view trees. maxElements counts only the
 * meaningful nodes that are published, so a huge or deeply nested tree of
 * unpublished containers would otherwise cost one binder read per node on the
 * service thread. Every visited node, the depth, and the wall-clock time since
 * the budget was created are capped here; exceeding any bound marks the
 * observation truncated.
 *
 * Capture stays on the accessibility service's main looper because provider
 * state (leases, generations, controller windows) is serialized there; the
 * time bound is what keeps one slow foreign tree from stalling that looper.
 */
internal class AccessibilityTraversalBudget(
    private val maxVisitedNodes: Int = ACCESSIBILITY_MAX_VISITED_NODES,
    private val maxDepth: Int = ACCESSIBILITY_MAX_TREE_DEPTH,
    maxDurationMs: Long = ACCESSIBILITY_MAX_TRAVERSAL_MS,
    private val nanoTime: () -> Long = System::nanoTime,
) {
    var visited: Int = 0
        private set

    private val deadlineNanos: Long

    init {
        require(maxVisitedNodes > 0) { "maxVisitedNodes must be positive" }
        require(maxDepth >= 0) { "maxDepth must be non-negative" }
        require(maxDurationMs > 0) { "maxDurationMs must be positive" }
        deadlineNanos = nanoTime() + maxDurationMs * 1_000_000L
    }

    /** Admits one node at [depth] (the root is depth 0); false once a bound is reached. */
    fun admit(depth: Int): Boolean {
        if (depth > maxDepth || visited >= maxVisitedNodes) return false
        if (nanoTime() - deadlineNanos >= 0) return false
        visited += 1
        return true
    }
}

internal const val ACCESSIBILITY_MAX_VISITED_NODES = 5_000
internal const val ACCESSIBILITY_MAX_TREE_DEPTH = 64
internal const val ACCESSIBILITY_MAX_TRAVERSAL_MS = 1_000L

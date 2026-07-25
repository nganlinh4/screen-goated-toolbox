package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.view.accessibility.AccessibilityNodeInfo
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService

internal enum class StableTargetResolutionKind {
    EXACT_PATH,
    UNIQUE_FINGERPRINT,
    NOT_FOUND,
    AMBIGUOUS,
    INCOMPLETE,
}

internal data class StableTargetResolution<T>(
    val node: T?,
    val kind: StableTargetResolutionKind,
    val visitedNodes: Int,
)

/**
 * Resolves a changing platform tree without weakening target identity.
 *
 * A moved path is accepted only when a complete bounded traversal finds one
 * exact fingerprint match. Incomplete or ambiguous trees remain stale.
 */
internal fun <T> resolveStableTarget(
    root: T,
    childPath: List<Int>,
    maxNodes: Int = MAX_RESOLUTION_NODES,
    childCount: (T) -> Int,
    childAt: (T, Int) -> T?,
    matches: (T) -> Boolean,
): StableTargetResolution<T> {
    require(maxNodes > 0)
    var pathNode: T? = root
    for (index in childPath) {
        val current = pathNode ?: break
        val count = platformRead { childCount(current) }
        if (count.failed || index !in 0 until (count.value ?: 0)) {
            pathNode = null
            break
        }
        val child = platformRead { childAt(current, index) }
        if (child.failed) {
            pathNode = null
            break
        }
        pathNode = child.value
    }
    val exactCandidate = pathNode
    if (exactCandidate != null) {
        val pathMatch = platformRead { matches(exactCandidate) }
        if (!pathMatch.failed && pathMatch.value == true) {
            return StableTargetResolution(
                node = exactCandidate,
                kind = StableTargetResolutionKind.EXACT_PATH,
                visitedNodes = childPath.size + 1,
            )
        }
    }

    val pending = ArrayDeque<T>()
    pending.addLast(root)
    var visited = 0
    var matched: T? = null
    while (pending.isNotEmpty()) {
        if (visited >= maxNodes) {
            return StableTargetResolution(null, StableTargetResolutionKind.INCOMPLETE, visited)
        }
        val node = pending.removeFirst()
        visited += 1
        val fingerprintMatch = platformRead { matches(node) }
        if (fingerprintMatch.failed) {
            return StableTargetResolution(null, StableTargetResolutionKind.INCOMPLETE, visited)
        }
        if (fingerprintMatch.value == true) {
            if (matched != null) {
                return StableTargetResolution(null, StableTargetResolutionKind.AMBIGUOUS, visited)
            }
            matched = node
        }
        val count = platformRead { childCount(node) }
        val resolvedChildCount = count.value
        if (count.failed || resolvedChildCount == null || resolvedChildCount < 0) {
            return StableTargetResolution(null, StableTargetResolutionKind.INCOMPLETE, visited)
        }
        if (resolvedChildCount > maxNodes - visited - pending.size) {
            return StableTargetResolution(null, StableTargetResolutionKind.INCOMPLETE, visited)
        }
        for (index in 0 until resolvedChildCount) {
            val child = platformRead { childAt(node, index) }
            if (child.failed) {
                return StableTargetResolution(null, StableTargetResolutionKind.INCOMPLETE, visited)
            }
            child.value?.let(pending::addLast)
        }
    }
    return if (matched == null) {
        StableTargetResolution(null, StableTargetResolutionKind.NOT_FOUND, visited)
    } else {
        StableTargetResolution(matched, StableTargetResolutionKind.UNIQUE_FINGERPRINT, visited)
    }
}

internal fun resolveAccessibilityNode(
    service: SgtAccessibilityService,
    lease: AccessibilityTargetLease,
): AccessibilityNodeInfo? {
    val root = findAccessibilityWindowRoot(
        service,
        lease.identity.displayId,
        lease.identity.windowId,
    ) ?: return null
    val resolution = resolveStableTarget(
        root = root,
        childPath = lease.childPath,
        childCount = { node -> node.childCount },
        childAt = { node, index -> node.getChild(index) },
        matches = { node -> node.matches(lease) },
    )
    when (resolution.kind) {
        StableTargetResolutionKind.UNIQUE_FINGERPRINT -> Log.i(
            TAG,
            "target_path_recovered target_id=${lease.id} " +
                "generation=${lease.identity.snapshotGeneration} " +
                "display_id=${lease.identity.displayId} window_id=${lease.identity.windowId} " +
                "visited_nodes=${resolution.visitedNodes}",
        )
        StableTargetResolutionKind.NOT_FOUND,
        StableTargetResolutionKind.AMBIGUOUS,
        StableTargetResolutionKind.INCOMPLETE,
        -> Log.d(
            TAG,
            "target_resolution_failed target_id=${lease.id} " +
                "generation=${lease.identity.snapshotGeneration} " +
                "display_id=${lease.identity.displayId} window_id=${lease.identity.windowId} " +
                "reason=${resolution.kind.name.lowercase()} " +
                "visited_nodes=${resolution.visitedNodes}",
        )
        StableTargetResolutionKind.EXACT_PATH -> Unit
    }
    return resolution.node
}

internal fun resolveAccessibilityNodeAtPath(
    service: SgtAccessibilityService,
    lease: AccessibilityTargetLease,
): AccessibilityNodeInfo? = resolveAccessibilityNodePath(service, lease).node

internal fun resolveAccessibilityNodePath(
    service: SgtAccessibilityService,
    lease: AccessibilityTargetLease,
): AccessibilityPathResolution {
    var node = findAccessibilityWindowRoot(
        service,
        lease.identity.displayId,
        lease.identity.windowId,
    ) ?: return AccessibilityPathResolution(node = null, platformReadFailed = false)
    for (index in lease.childPath) {
        val count = platformRead { node.childCount }
        if (count.failed) return failedPathResolution(lease)
        if (index !in 0 until (count.value ?: 0)) {
            return AccessibilityPathResolution(node = null, platformReadFailed = false)
        }
        val child = platformRead { node.getChild(index) }
        if (child.failed) return failedPathResolution(lease)
        node = child.value
            ?: return AccessibilityPathResolution(node = null, platformReadFailed = false)
    }
    return AccessibilityPathResolution(node = node, platformReadFailed = false)
}

internal fun AccessibilityNodeInfo.readChildCountSafely(): Int? =
    platformRead { childCount }.takeUnless { it.failed }?.value

internal fun AccessibilityNodeInfo.readChildSafely(index: Int): AccessibilityNodeRead {
    val read = platformRead { getChild(index) }
    return AccessibilityNodeRead(read.value, read.failed)
}

internal data class AccessibilityNodeRead(
    val node: AccessibilityNodeInfo?,
    val failed: Boolean,
)

internal data class AccessibilityPathResolution(
    val node: AccessibilityNodeInfo?,
    val platformReadFailed: Boolean,
)

private fun failedPathResolution(
    lease: AccessibilityTargetLease,
): AccessibilityPathResolution {
    Log.d(
        TAG,
        "target_path_read_failed target_id=${lease.id} " +
            "generation=${lease.identity.snapshotGeneration} " +
            "display_id=${lease.identity.displayId} window_id=${lease.identity.windowId}",
    )
    return AccessibilityPathResolution(node = null, platformReadFailed = true)
}

private inline fun <T> platformRead(block: () -> T): PlatformRead<T> = try {
    PlatformRead(block(), failed = false)
} catch (_: RuntimeException) {
    PlatformRead(value = null, failed = true)
}

private data class PlatformRead<T>(
    val value: T?,
    val failed: Boolean,
)

private const val MAX_RESOLUTION_NODES = 1_024
private const val TAG = "SGTPhoneControlAccessibility"

package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.view.Display
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AndroidSurfaceDescriptor
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AndroidSurfaceIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AndroidSurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.normalizeAndroidSurfaceKey
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal data class SurfaceContext(
    val observation: AccessibilityObservation,
    val lease: AndroidSurfaceLease,
) {
    fun snapshot(surface: AndroidSurfaceDescriptor): AccessibilityWindowSnapshot? =
        observation.windows.firstOrNull { window -> window.matches(surface.identity) }

    /**
     * Global navigation has no element or pointer target. Permit a retired content
     * generation only when Android still exposes the exact same platform window;
     * the caller must then validate foreground authority and create a fresh lease.
     */
    fun systemNavigationSnapshot(identity: AndroidSurfaceIdentity): AccessibilityWindowSnapshot? =
        observation.windows.singleOrNull { window -> window.matches(identity) }

    fun descriptor(window: AccessibilityWindowSnapshot): AndroidSurfaceDescriptor? =
        window.identity(observation.generation)?.let { identity ->
            AndroidSurfaceDescriptor(identity, window.title, window.active, window.focused)
        }

    fun resolveFocusedContinuation(target: AccessibilityWindowSnapshot): FocusedContinuation {
        val candidates = observation.windows.filter { window ->
            window.displayId == target.displayId &&
                window.packageName == target.packageName &&
                window.type == APPLICATION_WINDOW &&
                window.active && window.focused && !window.controllerOwned
        }
        val expectedTitle = target.title?.let(::normalizeAndroidSurfaceKey).orEmpty()
        val retained = candidates.filter { candidate ->
            candidate.id == target.id &&
                (expectedTitle.isEmpty() ||
                    candidate.title?.let(::normalizeAndroidSurfaceKey) == expectedTitle)
        }
        when (retained.size) {
            1 -> return FocusedContinuation.Resolved(retained.single())
            in 2..Int.MAX_VALUE -> return FocusedContinuation.Ambiguous(retained.descriptors())
        }
        if (expectedTitle.isEmpty()) return FocusedContinuation.NotFound
        val titled = candidates.filter { candidate ->
            candidate.title?.let(::normalizeAndroidSurfaceKey) == expectedTitle
        }
        return when (titled.size) {
            0 -> FocusedContinuation.NotFound
            1 -> FocusedContinuation.Resolved(titled.single())
            else -> FocusedContinuation.Ambiguous(titled.descriptors())
        }
    }

    fun canMinimize(target: AccessibilityWindowSnapshot, displayBounds: TargetBounds): Boolean {
        val displayWindows = observation.windows.filter { it.displayId == target.displayId }
        val applicationWindows = displayWindows.filter { window ->
            window.type == APPLICATION_WINDOW && !window.controllerOwned
        }
        val targetPackage = target.packageName ?: return false
        return target.displayId == Display.DEFAULT_DISPLAY &&
            target.focusTargetable() && target.active && target.focused &&
            !target.pictureInPicture && applicationWindows.isNotEmpty() &&
            applicationWindows.all { window -> window.packageName == targetPackage } &&
            applicationWindows.map(AccessibilityWindowSnapshot::bounds).covers(displayBounds) &&
            displayWindows.none { it.type == SPLIT_SCREEN_DIVIDER || it.pictureInPicture }
    }

    fun windowJson(window: AccessibilityWindowSnapshot, appLabel: String?): JsonObject = buildJsonObject {
        val descriptor = descriptor(window)
        put("id", window.id)
        put("display_id", window.displayId)
        put("package", window.packageName.orEmpty())
        put("app_label", appLabel.orEmpty())
        put("title", window.title.orEmpty())
        put("type", window.type)
        put("layer", window.layer)
        put("active", window.active)
        put("focused", window.focused)
        put("content_accessible", window.contentAccessible)
        put("controller_owned", window.controllerOwned)
        put("picture_in_picture", window.pictureInPicture)
        put("targetable", window.focusTargetable())
        descriptor?.let { put("target", it.target) }
        put("bounds", buildJsonObject {
            put("left", window.bounds.left)
            put("top", window.bounds.top)
            put("right", window.bounds.right)
            put("bottom", window.bounds.bottom)
        })
    }

    private fun List<AccessibilityWindowSnapshot>.descriptors(): List<AndroidSurfaceDescriptor> =
        mapNotNull(::descriptor).sortedBy(AndroidSurfaceDescriptor::target)
}

internal sealed interface FocusedContinuation {
    data object NotFound : FocusedContinuation
    data class Resolved(val window: AccessibilityWindowSnapshot) : FocusedContinuation
    data class Ambiguous(val choices: List<AndroidSurfaceDescriptor>) : FocusedContinuation
}

internal fun AccessibilityObservation.surfaceLease(): AndroidSurfaceLease = AndroidSurfaceLease(
    generation = generation,
    surfaces = windows.mapNotNull { window ->
        window.identity(generation)?.let { identity ->
            AndroidSurfaceDescriptor(identity, window.title, window.active, window.focused)
        }
    },
)

internal fun AccessibilityWindowSnapshot.focusTargetable(): Boolean =
    id >= 0 && type == APPLICATION_WINDOW && !packageName.isNullOrBlank() && !controllerOwned

internal fun AccessibilityWindowSnapshot.systemNavigationTargetable(): Boolean =
    id >= 0 && !packageName.isNullOrBlank() && !controllerOwned && (active || focused)

private fun AccessibilityWindowSnapshot.identity(generation: Long): AndroidSurfaceIdentity? =
    runCatching {
        AndroidSurfaceIdentity(generation, displayId, id.toLong(), packageName.orEmpty())
    }.getOrNull()

private fun AccessibilityWindowSnapshot.matches(identity: AndroidSurfaceIdentity): Boolean =
    displayId == identity.displayId && id.toLong() == identity.windowId &&
        packageName.orEmpty() == identity.packageName

internal fun List<TargetBounds>.covers(target: TargetBounds): Boolean {
    if (isEmpty() || any { bounds -> !target.contains(bounds) }) return false
    val xCuts = (flatMap { bounds -> listOf(bounds.left, bounds.right) } +
        listOf(target.left, target.right)).distinct().sorted()
    for (index in 0 until xCuts.lastIndex) {
        val left = xCuts[index]
        val right = xCuts[index + 1]
        if (left < target.left || right > target.right || right <= left) continue
        val spans = asSequence()
            .filter { bounds -> bounds.left <= left && bounds.right >= right }
            .map { bounds -> bounds.top to bounds.bottom }
            .filter { (top, bottom) -> bottom > top }
            .sortedBy(Pair<Int, Int>::first)
            .toList()
        var coveredTo = target.top
        for ((top, bottom) in spans) {
            if (top > coveredTo) return false
            coveredTo = maxOf(coveredTo, bottom)
            if (coveredTo >= target.bottom) break
        }
        if (coveredTo < target.bottom) return false
    }
    return true
}

private fun TargetBounds.contains(other: TargetBounds): Boolean =
    other.left >= left && other.top >= top && other.right <= right && other.bottom <= bottom

internal const val APPLICATION_WINDOW = "application"
private const val SPLIT_SCREEN_DIVIDER = "split_screen_divider"

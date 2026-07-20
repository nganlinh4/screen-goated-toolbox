package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import java.text.Normalizer
import java.util.Locale

/** Observation-bound identity for one Android accessibility window. */
internal data class AndroidSurfaceIdentity(
    val generation: Long,
    val displayId: Int,
    val windowId: Long,
    val packageName: String,
) {
    init {
        require(generation > 0L) { "generation must be positive" }
        require(displayId >= 0) { "display id must be non-negative" }
        require(windowId >= 0L) { "window id must be non-negative" }
        require(packageName.none { it == TARGET_SEPARATOR || it.isWhitespace() }) {
            "package name cannot contain target delimiters or whitespace"
        }
    }

    fun stableTarget(): String = buildString {
        append(STABLE_TARGET_NAMESPACE)
        append(TARGET_VERSION)
        append(TARGET_SEPARATOR).append(generation)
        append(TARGET_SEPARATOR).append(displayId)
        append(TARGET_SEPARATOR).append(windowId)
        append(TARGET_SEPARATOR).append(packageName)
    }

    internal companion object {
        fun parseStableTarget(value: String): AndroidSurfaceTargetParseResult =
            parseAndroidSurfaceTarget(value)
    }
}

internal data class AndroidSurfaceDescriptor(
    val identity: AndroidSurfaceIdentity,
    val title: String?,
    val active: Boolean,
    val focused: Boolean,
) {
    val target: String
        get() = identity.stableTarget()
}

/**
 * Immutable view of one accessibility observation. Stable targets resolve only
 * inside this lease; carrying one into a later generation fails closed.
 */
internal class AndroidSurfaceLease(
    val generation: Long,
    surfaces: List<AndroidSurfaceDescriptor>,
) {
    val surfaces: List<AndroidSurfaceDescriptor> = surfaces.toList()

    init {
        require(generation > 0L) { "generation must be positive" }
        require(this.surfaces.all { it.identity.generation == generation }) {
            "every surface must belong to the lease generation"
        }
        require(this.surfaces.map { it.identity }.distinct().size == this.surfaces.size) {
            "surface identities must be unique within a lease"
        }
    }

    fun resolve(target: String): AndroidSurfaceResolution {
        return when (val parsed = parseAndroidSurfaceTarget(target)) {
            AndroidSurfaceTargetParseResult.NamedTarget -> resolveHumanTarget(target)
            is AndroidSurfaceTargetParseResult.Malformed ->
                AndroidSurfaceResolution.Rejected(parsed.error)
            is AndroidSurfaceTargetParseResult.Stable -> resolveStableTarget(parsed.identity)
        }
    }

    private fun resolveStableTarget(identity: AndroidSurfaceIdentity): AndroidSurfaceResolution {
        if (identity.generation != generation) {
            return rejected(
                AndroidSurfaceResolutionError.StaleGeneration(
                    targetGeneration = identity.generation,
                    currentGeneration = generation,
                ),
            )
        }
        surfaces.singleOrNull { it.identity == identity }?.let {
            return AndroidSurfaceResolution.Resolved(it)
        }

        val samePhysicalWindow = surfaces.filter {
            it.identity.displayId == identity.displayId &&
                it.identity.windowId == identity.windowId
        }
        if (samePhysicalWindow.isNotEmpty()) {
            return rejected(
                AndroidSurfaceResolutionError.WrongPackage(
                    expectedPackage = identity.packageName,
                    currentPackages = samePhysicalWindow.map { it.identity.packageName }.stableStrings(),
                ),
            )
        }

        val sameLogicalWindow = surfaces.filter {
            it.identity.windowId == identity.windowId &&
                it.identity.packageName == identity.packageName
        }
        if (sameLogicalWindow.isNotEmpty()) {
            return rejected(
                AndroidSurfaceResolutionError.WrongDisplay(
                    expectedDisplay = identity.displayId,
                    currentDisplays = sameLogicalWindow.map { it.identity.displayId }.distinct().sorted(),
                ),
            )
        }

        val reusedWindowId = surfaces.filter { it.identity.windowId == identity.windowId }
        if (reusedWindowId.isNotEmpty()) {
            return rejected(
                AndroidSurfaceResolutionError.ReusedWindowId(
                    windowId = identity.windowId,
                    currentTargets = reusedWindowId.stableTargets(),
                ),
            )
        }
        return rejected(AndroidSurfaceResolutionError.StableTargetNotFound(identity.stableTarget()))
    }

    private fun resolveHumanTarget(target: String): AndroidSurfaceResolution {
        val normalizedTarget = normalizeAndroidSurfaceKey(target)
        if (normalizedTarget.isEmpty()) {
            return rejected(AndroidSurfaceResolutionError.EmptyTarget)
        }
        val matches = surfaces.filter { surface ->
            normalizeAndroidSurfaceKey(surface.identity.packageName) == normalizedTarget ||
                surface.title?.let(::normalizeAndroidSurfaceKey) == normalizedTarget
        }
        return when (matches.size) {
            0 -> rejected(AndroidSurfaceResolutionError.NamedTargetNotFound(target))
            1 -> AndroidSurfaceResolution.Resolved(matches.single())
            else -> rejected(
                AndroidSurfaceResolutionError.Ambiguous(
                    target = target,
                    choices = matches.stableTargets(),
                ),
            )
        }
    }
}

internal sealed interface AndroidSurfaceTargetParseResult {
    data object NamedTarget : AndroidSurfaceTargetParseResult

    data class Stable(val identity: AndroidSurfaceIdentity) : AndroidSurfaceTargetParseResult

    data class Malformed(
        val error: AndroidSurfaceResolutionError.MalformedStableTarget,
    ) : AndroidSurfaceTargetParseResult
}

internal sealed interface AndroidSurfaceResolution {
    data class Resolved(val surface: AndroidSurfaceDescriptor) : AndroidSurfaceResolution

    data class Rejected(val error: AndroidSurfaceResolutionError) : AndroidSurfaceResolution
}

internal sealed interface AndroidSurfaceResolutionError {
    val code: String

    data object EmptyTarget : AndroidSurfaceResolutionError {
        override val code: String = "invalid_target"
    }

    data class MalformedStableTarget(val target: String) : AndroidSurfaceResolutionError {
        override val code: String = "invalid_target"
    }

    data class StaleGeneration(
        val targetGeneration: Long,
        val currentGeneration: Long,
    ) : AndroidSurfaceResolutionError {
        override val code: String = "stale_target"
    }

    data class WrongPackage(
        val expectedPackage: String,
        val currentPackages: List<String>,
    ) : AndroidSurfaceResolutionError {
        override val code: String = "stale_target"
    }

    data class WrongDisplay(
        val expectedDisplay: Int,
        val currentDisplays: List<Int>,
    ) : AndroidSurfaceResolutionError {
        override val code: String = "stale_target"
    }

    data class ReusedWindowId(
        val windowId: Long,
        val currentTargets: List<String>,
    ) : AndroidSurfaceResolutionError {
        override val code: String = "stale_target"
    }

    data class StableTargetNotFound(val target: String) : AndroidSurfaceResolutionError {
        override val code: String = "stale_target"
    }

    data class NamedTargetNotFound(val target: String) : AndroidSurfaceResolutionError {
        override val code: String = "target_not_found"
    }

    data class Ambiguous(
        val target: String,
        val choices: List<String>,
    ) : AndroidSurfaceResolutionError {
        override val code: String = "ambiguous_target"
    }
}

internal fun normalizeAndroidSurfaceKey(value: String): String {
    val folded = Normalizer.normalize(value, Normalizer.Form.NFKC).lowercase(Locale.ROOT)
    val result = StringBuilder(folded.length)
    var pendingSpace = false
    folded.codePoints().forEach { codePoint ->
        if (Character.isWhitespace(codePoint) || Character.isSpaceChar(codePoint)) {
            pendingSpace = result.isNotEmpty()
        } else {
            if (pendingSpace) result.append(' ')
            result.appendCodePoint(codePoint)
            pendingSpace = false
        }
    }
    return result.toString()
}

private fun parseAndroidSurfaceTarget(value: String): AndroidSurfaceTargetParseResult {
    val trimmed = value.trim()
    if (!trimmed.startsWith(STABLE_TARGET_NAMESPACE)) {
        return AndroidSurfaceTargetParseResult.NamedTarget
    }
    val parts = trimmed.removePrefix(STABLE_TARGET_NAMESPACE).split(TARGET_SEPARATOR, limit = 5)
    if (parts.size != 5 || parts[0] != TARGET_VERSION) return malformed(value)
    val generation = parts[1].canonicalLong(minimum = 1L) ?: return malformed(value)
    val displayId = parts[2].canonicalInt(minimum = 0) ?: return malformed(value)
    val windowId = parts[3].canonicalLong(minimum = 0L) ?: return malformed(value)
    val packageName = parts[4]
    val identity = runCatching {
        AndroidSurfaceIdentity(generation, displayId, windowId, packageName)
    }.getOrNull() ?: return malformed(value)
    return AndroidSurfaceTargetParseResult.Stable(identity)
}

private fun String.canonicalLong(minimum: Long): Long? {
    val parsed = toLongOrNull() ?: return null
    return parsed.takeIf { it >= minimum && it.toString() == this }
}

private fun String.canonicalInt(minimum: Int): Int? {
    val parsed = toIntOrNull() ?: return null
    return parsed.takeIf { it >= minimum && it.toString() == this }
}

private fun malformed(target: String) = AndroidSurfaceTargetParseResult.Malformed(
    AndroidSurfaceResolutionError.MalformedStableTarget(target),
)

private fun rejected(error: AndroidSurfaceResolutionError) =
    AndroidSurfaceResolution.Rejected(error)

private fun List<AndroidSurfaceDescriptor>.stableTargets(): List<String> =
    map(AndroidSurfaceDescriptor::target).distinct().sorted()

private fun List<String>.stableStrings(): List<String> = distinct().sorted()

private const val STABLE_TARGET_NAMESPACE = "@android-window:"
private const val TARGET_VERSION = "v1"
private const val TARGET_SEPARATOR = ':'

package dev.screengoated.toolbox.mobile.phonecontrol.provider.visual

import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.tools.AccessibilityGridIdentity
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlin.math.roundToInt

internal sealed interface VisualProviderResult<out T> {
    data class Success<T>(val value: T) : VisualProviderResult<T>

    data class Failure(
        val code: String,
        val message: String,
        val retryable: Boolean,
        val requiredUserStep: String? = null,
        val freshObservationRequired: Boolean = false,
    ) : VisualProviderResult<Nothing>
}

internal enum class VisualViewKind(val wireName: String) {
    ACTIVE_SURFACE("active_surface"),
    WHOLE_DISPLAY("whole_display"),
    ZOOM("zoom"),
}

internal data class VisualGridIdentity(
    val observationGeneration: Long,
    val visualRevision: Long,
    val displayId: Int,
    val windowId: Long?,
    val packageOrSurface: String,
    val bounds: TargetBounds,
    val surfaceLease: AccessibilitySurfaceLease? = null,
    val rotation: Int,
    val densityDpi: Int,
    val capturedAtMs: Long,
    val columns: Int = GRID_COLUMNS,
    val rows: Int = GRID_ROWS,
) {
    init {
        require(observationGeneration > 0)
        require(visualRevision > 0)
        require(displayId >= 0)
        require(windowId == null || windowId >= 0)
        require(packageOrSurface.isNotBlank())
        require(densityDpi > 0)
        require(capturedAtMs >= 0)
        require(columns > 0 && rows > 0)
    }

    val wireIdentity: String
        get() = listOf(
            observationGeneration,
            visualRevision,
            displayId,
            windowId ?: "display",
            packageOrSurface,
            bounds.left,
            bounds.top,
            bounds.right,
            bounds.bottom,
            rotation,
            densityDpi,
            capturedAtMs,
            surfaceLease?.windowLayer ?: "display",
            surfaceLease?.authority?.wireName ?: "display",
            surfaceLease?.controllerOwned ?: false,
            columns,
            rows,
        ).joinToString(":")

    fun cellBounds(cell: Int, paddingCells: Double = 0.0): TargetBounds? {
        if (cell !in 1..columns * rows || paddingCells < 0.0) return null
        val index = cell - 1
        val column = index % columns
        val row = index / columns
        val width = bounds.right - bounds.left
        val height = bounds.bottom - bounds.top
        val cellWidth = width.toDouble() / columns
        val cellHeight = height.toDouble() / rows
        return TargetBounds(
            (bounds.left + (column - paddingCells) * cellWidth)
                .roundToInt().coerceIn(bounds.left, bounds.right),
            (bounds.top + (row - paddingCells) * cellHeight)
                .roundToInt().coerceIn(bounds.top, bounds.bottom),
            (bounds.left + (column + 1.0 + paddingCells) * cellWidth)
                .roundToInt().coerceIn(bounds.left, bounds.right),
            (bounds.top + (row + 1.0 + paddingCells) * cellHeight)
                .roundToInt().coerceIn(bounds.top, bounds.bottom),
        ).takeIf { it.right - it.left >= MIN_CROP_SIDE && it.bottom - it.top >= MIN_CROP_SIDE }
    }

    fun asAccessibilityGrid(): AccessibilityGridIdentity? {
        val exactWindowId = windowId ?: return null
        val lease = surfaceLease ?: return null
        return AccessibilityGridIdentity(
            observationGeneration = observationGeneration,
            visualRevision = visualRevision,
            displayId = displayId,
            windowId = exactWindowId,
            bounds = bounds,
            columns = columns,
            rows = rows,
            surfaceLease = lease,
            rotation = rotation,
            densityDpi = densityDpi,
            capturedAtMs = capturedAtMs,
        )
    }
}

internal data class VisualFrameIdentity(
    val observationGeneration: Long,
    val visualRevision: Long,
    val displayId: Int,
    val windowId: Long?,
    val packageOrSurface: String,
    val cropBounds: TargetBounds,
    val captureWidth: Int,
    val captureHeight: Int,
    val rotation: Int,
    val densityDpi: Int,
    val capturedAtMs: Long,
    val viewKind: VisualViewKind,
    val clean: Boolean,
    val grid: VisualGridIdentity?,
) {
    init {
        require(observationGeneration > 0)
        require(visualRevision > 0)
        require(displayId >= 0)
        require(windowId == null || windowId >= 0)
        require(packageOrSurface.isNotBlank())
        require(captureWidth > 0 && captureHeight > 0)
        require(capturedAtMs >= 0)
        require(clean == (grid == null)) { "clean frames must not carry grid identity" }
    }

    val wireIdentity: String
        get() = listOf(
            observationGeneration,
            visualRevision,
            displayId,
            windowId ?: "display",
            cropBounds.left,
            cropBounds.top,
            cropBounds.right,
            cropBounds.bottom,
            rotation,
            densityDpi,
            capturedAtMs,
            viewKind.wireName,
            if (clean) "clean" else "grid",
        ).joinToString(":")

    fun toWireJson(): JsonObject = buildJsonObject {
        put("identity", wireIdentity)
        put("observation_generation", observationGeneration)
        put("visual_revision", visualRevision)
        put("display_id", displayId)
        windowId?.let { put("window_id", it) }
        put("package_or_surface", packageOrSurface)
        put("screen_crop_bounds", cropBounds.toWireJson())
        put("capture_width", captureWidth)
        put("capture_height", captureHeight)
        put("rotation", rotation)
        put("density_dpi", densityDpi)
        put("captured_at_ms", capturedAtMs)
        put("view", viewKind.wireName)
        put("clean", clean)
        grid?.let {
            put("grid_identity", it.wireIdentity)
            put("grid_columns", it.columns)
            put("grid_rows", it.rows)
        }
    }
}

internal data class VisualFrame(
    val identity: VisualFrameIdentity,
    val screenPayload: String,
) {
    init {
        require(screenPayload.isNotBlank())
    }
}

internal const val GRID_COLUMNS = 6
internal const val GRID_ROWS = 5
internal const val ZOOM_PADDING_CELLS = 0.25
private const val MIN_CROP_SIDE = 8

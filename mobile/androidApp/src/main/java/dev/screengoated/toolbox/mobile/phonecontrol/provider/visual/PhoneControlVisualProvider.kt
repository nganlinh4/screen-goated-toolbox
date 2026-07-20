package dev.screengoated.toolbox.mobile.phonecontrol.provider.visual

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.os.Build
import android.os.SystemClock
import android.view.Display
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityScreenshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.ACTIVE_CONTENT_WINDOW_TYPE
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.surfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import dev.screengoated.toolbox.mobile.phonecontrol.session.buildPhoneControlScreenPayload
import dev.screengoated.toolbox.mobile.phonecontrol.tools.AccessibilityGridIdentity
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max

internal object PhoneControlVisualProvider {
    private val captureMutex = Mutex()
    private var selection: ViewSelection = ViewSelection.ActiveSurface

    private var cachedScreenshot: AccessibilityScreenshot? = null

    @Volatile
    private var latestGrid: VisualGridIdentity? = null

    val observationGeneration: Long
        get() = PhoneControlAccessibilityProvider.observationGeneration

    fun currentAccessibilityGrid(): AccessibilityGridIdentity? = latestGrid
        ?.takeIf { it.visualRevision == PhoneControlAccessibilityProvider.currentVisualRevision }
        ?.asAccessibilityGrid()

    suspend fun captureStreamingFrame(): VisualProviderResult<VisualFrame> =
        captureMutex.withLock { captureLocked(clean = false, resetStaleZoom = true) }

    suspend fun resetView(): VisualProviderResult<VisualFrame> = captureMutex.withLock {
        selection = ViewSelection.ActiveSurface
        captureLocked(clean = false, resetStaleZoom = false)
    }

    suspend fun seeWholeScreen(): VisualProviderResult<VisualFrame> = captureMutex.withLock {
        selection = ViewSelection.WholeDisplay
        captureLocked(clean = false, resetStaleZoom = false)
    }

    suspend fun look(): VisualProviderResult<VisualFrame> = captureMutex.withLock {
        captureLocked(clean = true, resetStaleZoom = true)
    }

    suspend fun zoom(cell: Int): VisualProviderResult<VisualFrame> = captureMutex.withLock {
        val grid = latestGrid
            ?.takeIf { it.visualRevision == PhoneControlAccessibilityProvider.currentVisualRevision }
            ?: return@withLock staleFrame(
            "There is no current numbered visual frame. Capture a fresh view first.",
        )
        if (grid.observationGeneration != observationGeneration) {
            return@withLock staleFrame("The numbered frame changed before zoom was requested.")
        }
        val crop = grid.cellBounds(cell, ZOOM_PADDING_CELLS)
            ?: return@withLock VisualProviderResult.Failure(
                code = "invalid_arguments",
                message = "Cell must be between 1 and ${grid.columns * grid.rows}.",
                retryable = false,
            )
        val previous = selection
        selection = ViewSelection.Zoom(
            BoundView(
                observationGeneration = grid.observationGeneration,
                visualRevision = grid.visualRevision,
                displayId = grid.displayId,
                windowId = grid.windowId,
                packageOrSurface = grid.packageOrSurface,
                bounds = crop,
                surfaceLease = grid.surfaceLease,
            ),
        )
        captureLocked(clean = false, resetStaleZoom = false).also { result ->
            if (result is VisualProviderResult.Failure) selection = previous
        }
    }

    private suspend fun captureLocked(
        clean: Boolean,
        resetStaleZoom: Boolean,
    ): VisualProviderResult<VisualFrame> {
        var result = captureOnce(clean, resetStaleZoom)
        repeat(MAX_STALE_CAPTURE_RETRIES) {
            if (result !is VisualProviderResult.Failure || result.code != "stale_frame") {
                return result
            }
            delay(STALE_CAPTURE_RETRY_DELAY_MS)
            result = captureOnce(clean, resetStaleZoom)
        }
        return result
    }

    private suspend fun captureOnce(
        clean: Boolean,
        resetStaleZoom: Boolean,
    ): VisualProviderResult<VisualFrame> {
        val observation = when (val observed = PhoneControlAccessibilityProvider.observeForVisual()) {
            is AccessibilityProviderResult.Failure -> return observed.toVisualFailure()
            is AccessibilityProviderResult.Success -> observed.value
        }
        val view = when (val chosen = resolveView(observation, selection)) {
            is VisualProviderResult.Failure -> {
                if (resetStaleZoom && selection is ViewSelection.Zoom && chosen.code == "stale_frame") {
                    selection = ViewSelection.ActiveSurface
                    when (val reset = resolveView(observation, selection)) {
                        is VisualProviderResult.Failure -> return reset
                        is VisualProviderResult.Success -> reset.value
                    }
                } else {
                    return chosen
                }
            }
            is VisualProviderResult.Success -> chosen.value
        }
        if (view.displayId != Display.DEFAULT_DISPLAY) {
            return VisualProviderResult.Failure(
                code = "unsupported_display",
                message = "Accessibility screenshots currently capture only the default display.",
                retryable = false,
            )
        }
        val captureWindow = view.windowId?.let { windowId ->
            observation.windows.singleOrNull { it.id.toLong() == windowId }
        }
        val screenshot = when (
            val captured = screenshotFor(
                observationGeneration = observation.generation,
                windowId = captureWindow?.id?.toLong(),
                windowBounds = captureWindow?.bounds,
            )
        ) {
            is AccessibilityProviderResult.Failure -> return captured.toVisualFailure()
            is AccessibilityProviderResult.Success -> captured.value
        }
        try {
            if (screenshot.generation != observation.generation ||
                observationGeneration != observation.generation
            ) {
                return staleFrame("The visible surface changed during screenshot capture.")
            }
            val requestedBounds = view.bounds ?: screenshot.captureBounds
            val crop = mapCaptureCrop(
                requestedBounds,
                screenshot.captureBounds,
                screenshot.bitmap.width,
                screenshot.bitmap.height,
            )
                ?: return VisualProviderResult.Failure(
                    code = "surface_outside_capture",
                    message = "The requested view is outside the captured default display.",
                    retryable = true,
                    freshObservationRequired = true,
                )
            val sourceCrop = screenshot.bitmap.crop(crop.bitmapBounds)
            val outgoing = if (clean) sourceCrop else drawGrid(sourceCrop, observation.densityDpi)
            return try {
                val visualGrid = if (clean) {
                    null
                } else {
                    VisualGridIdentity(
                        observationGeneration = observation.generation,
                        visualRevision = screenshot.visualRevision,
                        displayId = view.displayId,
                        windowId = view.windowId,
                        packageOrSurface = view.packageOrSurface,
                        bounds = crop.absoluteBounds,
                        surfaceLease = view.surfaceLease,
                        rotation = observation.displayRotation,
                        densityDpi = observation.densityDpi,
                        capturedAtMs = screenshot.capturedAtMs,
                    )
                }
                val identity = VisualFrameIdentity(
                    observationGeneration = observation.generation,
                    visualRevision = screenshot.visualRevision,
                    displayId = view.displayId,
                    windowId = view.windowId,
                    packageOrSurface = view.packageOrSurface,
                    cropBounds = crop.absoluteBounds,
                    captureWidth = screenshot.bitmap.width,
                    captureHeight = screenshot.bitmap.height,
                    rotation = observation.displayRotation,
                    densityDpi = observation.densityDpi,
                    capturedAtMs = screenshot.capturedAtMs,
                    viewKind = view.kind,
                    clean = clean,
                    grid = visualGrid,
                )
                if (visualGrid != null &&
                    visualGrid.visualRevision == PhoneControlAccessibilityProvider.currentVisualRevision
                ) {
                    latestGrid = visualGrid
                }
                VisualProviderResult.Success(
                    VisualFrame(identity, buildPhoneControlScreenPayload(outgoing)),
                )
            } finally {
                if (outgoing !== sourceCrop) outgoing.recycle()
                if (sourceCrop !== screenshot.bitmap) sourceCrop.recycle()
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Throwable) {
            return VisualProviderResult.Failure(
                code = "screenshot_processing_failed",
                message = error.message ?: "The current screenshot could not be prepared.",
                retryable = true,
            )
        }
    }

    private suspend fun screenshotFor(
        observationGeneration: Long,
        windowId: Long?,
        windowBounds: TargetBounds?,
    ): AccessibilityProviderResult<AccessibilityScreenshot> {
        val existing = cachedScreenshot
        if (existing != null && (
                existing.generation != observationGeneration ||
                    existing.visualRevision != PhoneControlAccessibilityProvider.currentVisualRevision ||
                    existing.windowId != windowId ||
                    existing.captureBounds != windowBounds && windowBounds != null
                )
        ) {
            existing.bitmap.recycle()
            cachedScreenshot = null
        }
        val reusable = cachedScreenshot?.takeIf { screenshot ->
            !screenshot.bitmap.isRecycled && shouldReuseVisualScreenshot(
                capturedGeneration = screenshot.generation,
                capturedAtMs = screenshot.capturedAtMs,
                requestedGeneration = observationGeneration,
                nowMs = SystemClock.elapsedRealtime(),
                apiLevel = Build.VERSION.SDK_INT,
            )
        }
        if (reusable != null) return AccessibilityProviderResult.Success(reusable)

        return when (
            val captured = PhoneControlAccessibilityProvider.screenshot(windowId, windowBounds)
        ) {
            is AccessibilityProviderResult.Success -> {
                cachedScreenshot?.bitmap?.let { bitmap ->
                    if (!bitmap.isRecycled) bitmap.recycle()
                }
                cachedScreenshot = captured.value
                captured
            }
            is AccessibilityProviderResult.Failure -> {
                val fallback = cachedScreenshot?.takeIf { screenshot ->
                    captured.code == "screenshot_rate_limited" &&
                        screenshot.generation == observationGeneration &&
                        screenshot.visualRevision ==
                        PhoneControlAccessibilityProvider.currentVisualRevision &&
                        !screenshot.bitmap.isRecycled
                }
                if (fallback != null) {
                    AccessibilityProviderResult.Success(fallback)
                } else {
                    captured
                }
            }
        }
    }

    private const val MAX_STALE_CAPTURE_RETRIES = 2
    private const val STALE_CAPTURE_RETRY_DELAY_MS = 40L
}

internal fun shouldReuseVisualScreenshot(
    capturedGeneration: Long,
    capturedAtMs: Long,
    requestedGeneration: Long,
    nowMs: Long,
    apiLevel: Int,
): Boolean {
    if (capturedGeneration != requestedGeneration || nowMs < capturedAtMs) return false
    return nowMs - capturedAtMs <= visualScreenshotReuseWindowMs(apiLevel)
}

internal fun visualScreenshotReuseWindowMs(apiLevel: Int): Long =
    if (apiLevel == Build.VERSION_CODES.R) 1_000L else 333L

private sealed interface ViewSelection {
    data object ActiveSurface : ViewSelection
    data object WholeDisplay : ViewSelection
    data class Zoom(val binding: BoundView) : ViewSelection
}

private data class BoundView(
    val observationGeneration: Long,
    val visualRevision: Long,
    val displayId: Int,
    val windowId: Long?,
    val packageOrSurface: String,
    val bounds: TargetBounds,
    val surfaceLease: AccessibilitySurfaceLease?,
)

private data class ResolvedView(
    val displayId: Int,
    val windowId: Long?,
    val packageOrSurface: String,
    val bounds: TargetBounds?,
    val surfaceLease: AccessibilitySurfaceLease?,
    val kind: VisualViewKind,
)

private fun resolveView(
    observation: AccessibilityObservation,
    selection: ViewSelection,
): VisualProviderResult<ResolvedView> = when (selection) {
    is ViewSelection.ActiveSurface -> activeView(observation)
    is ViewSelection.WholeDisplay -> VisualProviderResult.Success(
        ResolvedView(
            displayId = Display.DEFAULT_DISPLAY,
            windowId = null,
            packageOrSurface = "android-display-${Display.DEFAULT_DISPLAY}",
            bounds = null,
            surfaceLease = null,
            kind = VisualViewKind.WHOLE_DISPLAY,
        ),
    )
    is ViewSelection.Zoom -> {
        val binding = selection.binding
        if (!binding.matches(observation)) {
            staleFrame("The source frame changed before its zoom crop could be captured.")
        } else {
            VisualProviderResult.Success(
                ResolvedView(
                    binding.displayId,
                    binding.windowId,
                    binding.packageOrSurface,
                    binding.bounds,
                    binding.surfaceLease,
                    VisualViewKind.ZOOM,
                ),
            )
        }
    }
}

private fun activeView(observation: AccessibilityObservation): VisualProviderResult<ResolvedView> {
    val window = selectVisualSurface(observation.windows)
        ?: return VisualProviderResult.Failure(
            code = "surface_unavailable",
            message = "No external visual surface is currently observable.",
            retryable = true,
            freshObservationRequired = true,
        )
    return VisualProviderResult.Success(
        ResolvedView(
            displayId = window.displayId,
            windowId = window.id.toLong(),
            packageOrSurface = window.packageName?.takeIf(String::isNotBlank)
                ?: window.title?.takeIf(String::isNotBlank)
                ?: "android-window-${window.id}",
            bounds = window.bounds,
            surfaceLease = window.surfaceLease(observation.generation),
            kind = VisualViewKind.ACTIVE_SURFACE,
        ),
    )
}

internal fun selectVisualSurface(
    windows: List<AccessibilityWindowSnapshot>,
): AccessibilityWindowSnapshot? = windows
    .asSequence()
    .filter { window ->
        window.displayId == Display.DEFAULT_DISPLAY &&
            !window.controllerOwned &&
            window.type in VISUAL_SURFACE_WINDOW_TYPES
    }
    .sortedWith(
        compareByDescending<AccessibilityWindowSnapshot> { it.active || it.focused }
            .thenByDescending { it.active }
            .thenByDescending { it.focused }
            .thenByDescending { it.layer },
    )
    .firstOrNull()

private val VISUAL_SURFACE_WINDOW_TYPES = setOf(
    "application",
    "system",
    ACTIVE_CONTENT_WINDOW_TYPE,
)

private fun BoundView.matches(
    observation: AccessibilityObservation,
): Boolean {
    if (observation.generation != observationGeneration) return false
    if (visualRevision != PhoneControlAccessibilityProvider.currentVisualRevision) return false
    if (windowId == null) return displayId == Display.DEFAULT_DISPLAY
    return observation.windows.any { window ->
        window.id.toLong() == windowId &&
            window.displayId == displayId &&
            (window.packageName == packageOrSurface ||
                (window.packageName.isNullOrBlank() && window.title == packageOrSurface)) &&
            window.bounds.contains(bounds) &&
            (surfaceLease == null || window.surfaceLease(observation.generation) == surfaceLease)
    }
}

private fun AccessibilityProviderResult.Failure.toVisualFailure() = VisualProviderResult.Failure(
    code = code,
    message = message,
    retryable = retryable,
    requiredUserStep = requiredUserStep
        ?: if (code == "capability_unavailable") "enable_accessibility" else null,
    freshObservationRequired = freshObservationRequired,
)

private fun staleFrame(message: String) = VisualProviderResult.Failure(
    code = "stale_frame",
    message = message,
    retryable = true,
    freshObservationRequired = true,
)

internal data class CaptureCropMapping(
    val absoluteBounds: TargetBounds,
    val bitmapBounds: TargetBounds,
)

internal fun mapCaptureCrop(
    requested: TargetBounds,
    capture: TargetBounds,
    bitmapWidth: Int,
    bitmapHeight: Int,
): CaptureCropMapping? {
    if (bitmapWidth <= 0 || bitmapHeight <= 0) return null
    val absoluteLeft = maxOf(requested.left, capture.left)
    val absoluteTop = maxOf(requested.top, capture.top)
    val absoluteRight = minOf(requested.right, capture.right)
    val absoluteBottom = minOf(requested.bottom, capture.bottom)
    if (absoluteRight <= absoluteLeft || absoluteBottom <= absoluteTop) return null
    val absolute = TargetBounds(absoluteLeft, absoluteTop, absoluteRight, absoluteBottom)
    val captureWidth = capture.right - capture.left
    val captureHeight = capture.bottom - capture.top
    if (captureWidth <= 0 || captureHeight <= 0) return null
    val scaleX = bitmapWidth.toDouble() / captureWidth
    val scaleY = bitmapHeight.toDouble() / captureHeight
    val local = TargetBounds(
        floor((absolute.left - capture.left) * scaleX).toInt().coerceIn(0, bitmapWidth),
        floor((absolute.top - capture.top) * scaleY).toInt().coerceIn(0, bitmapHeight),
        ceil((absolute.right - capture.left) * scaleX).toInt().coerceIn(0, bitmapWidth),
        ceil((absolute.bottom - capture.top) * scaleY).toInt().coerceIn(0, bitmapHeight),
    ).takeIf { it.right > it.left && it.bottom > it.top } ?: return null
    return CaptureCropMapping(absolute, local)
}

private fun TargetBounds.contains(other: TargetBounds): Boolean =
    other.left >= left && other.top >= top && other.right <= right && other.bottom <= bottom

private fun Bitmap.crop(bounds: TargetBounds): Bitmap = Bitmap.createBitmap(
    this,
    bounds.left,
    bounds.top,
    bounds.right - bounds.left,
    bounds.bottom - bounds.top,
)

private fun drawGrid(source: Bitmap, densityDpi: Int): Bitmap {
    val output = source.copy(Bitmap.Config.ARGB_8888, true)
        ?: error("Could not allocate visual grounding frame")
    val canvas = Canvas(output)
    val cellWidth = output.width.toFloat() / GRID_COLUMNS
    val cellHeight = output.height.toFloat() / GRID_ROWS
    val line = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.MAGENTA
        alpha = 56
        strokeWidth = max(1f, densityDpi / 240f)
    }
    repeat(GRID_COLUMNS - 1) { column ->
        val x = (column + 1) * cellWidth
        canvas.drawLine(x, 0f, x, output.height.toFloat(), line)
    }
    repeat(GRID_ROWS - 1) { row ->
        val y = (row + 1) * cellHeight
        canvas.drawLine(0f, y, output.width.toFloat(), y, line)
    }
    val textSize = (minOf(cellWidth, cellHeight) * 0.20f).coerceIn(18f, 54f)
    val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.YELLOW
        typeface = Typeface.DEFAULT_BOLD
        this.textSize = textSize
    }
    val plate = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.argb(190, 0, 0, 0) }
    repeat(GRID_ROWS) { row ->
        repeat(GRID_COLUMNS) { column ->
            val label = (row * GRID_COLUMNS + column + 1).toString()
            val left = column * cellWidth + textSize * 0.14f
            val top = row * cellHeight + textSize * 0.10f
            val width = text.measureText(label)
            canvas.drawRect(
                left,
                top,
                left + width + textSize * 0.28f,
                top + textSize * 1.16f,
                plate,
            )
            canvas.drawText(label, left + textSize * 0.14f, top + textSize * 0.92f, text)
        }
    }
    return output
}

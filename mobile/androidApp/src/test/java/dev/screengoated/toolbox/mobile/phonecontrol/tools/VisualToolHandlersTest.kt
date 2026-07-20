package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.VisualFrame
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.VisualFrameIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.VisualGridIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.VisualProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.VisualViewKind
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class VisualToolHandlersTest {
    @Test
    fun `grid cell geometry matches the numbered frame including zoom context`() {
        val grid = grid(bounds = TargetBounds(0, 0, 600, 500))

        assertEquals(TargetBounds(75, 75, 225, 225), grid.cellBounds(8, 0.25))
        assertEquals(TargetBounds(0, 0, 125, 125), grid.cellBounds(1, 0.25))
        assertNull(grid.cellBounds(0, 0.25))
        assertNull(grid.cellBounds(31, 0.25))
    }

    @Test
    fun `whole-display grid cannot become an application input grid`() {
        val whole = grid(windowId = null, packageOrSurface = "android-display-0")

        assertNull(whole.asAccessibilityGrid())
    }

    @Test
    fun `visual tools attach their exact frame for response then evidence delivery`() = runTest {
        val backend = FakeVisualBackend()
        val handlers = VisualToolHandlers(backend)
        val reset = frame(kind = VisualViewKind.ACTIVE_SURFACE, payload = "reset-frame")
        val whole = frame(kind = VisualViewKind.WHOLE_DISPLAY, payload = "whole-frame", windowId = null)
        val zoom = frame(kind = VisualViewKind.ZOOM, payload = "zoom-frame")
        backend.reset = VisualProviderResult.Success(reset)
        backend.whole = VisualProviderResult.Success(whole)
        backend.zoomed = VisualProviderResult.Success(zoom)

        val resetResult = handlers.resetView(JOB)
        val wholeResult = handlers.seeWholeScreen(JOB)
        val zoomResult = handlers.zoom(JOB, args("cell" to 17))

        assertSame(reset.screenPayload, resetResult.screenFramePayload)
        assertSame(whole.screenPayload, wholeResult.screenFramePayload)
        assertSame(zoom.screenPayload, zoomResult.screenFramePayload)
        assertEquals(listOf(17), backend.zoomCells)
        assertEquals("zoom", zoomResult.response.getValue("frame").jsonObject.string("view"))
        assertEquals(23L, zoomResult.response.getValue("frame").jsonObject.long("visual_revision"))
        assertEquals(17L, zoomResult.response.long("observation_generation"))
        assertEquals("proven_no_effect", zoomResult.response.string("effect_status"))
        assertFalse(zoomResult.mutating)
    }

    @Test
    fun `visual revision participates in both frame and grid wire identity`() {
        val first = frame().identity
        val changed = first.copy(
            visualRevision = 24,
            grid = requireNotNull(first.grid).copy(visualRevision = 24),
        )

        assertTrue(":23:" in first.wireIdentity)
        assertTrue(":23:" in requireNotNull(first.grid).wireIdentity)
        assertEquals(23L, first.toWireJson().long("visual_revision"))
        assertFalse(first.wireIdentity == changed.wireIdentity)
        assertFalse(requireNotNull(first.grid).wireIdentity == requireNotNull(changed.grid).wireIdentity)
    }

    @Test
    fun `look carries the clean frame and leaves visual meaning to the live model`() = runTest {
        val backend = FakeVisualBackend()
        val clean = frame(kind = VisualViewKind.ZOOM, payload = "clean-frame", clean = true)
        backend.looked = VisualProviderResult.Success(clean)

        val result = VisualToolHandlers(backend).look(
            JOB,
            args("question" to "Which icon is selected?"),
        )

        assertEquals("clean-frame", result.screenFramePayload)
        assertEquals("Which icon is selected?", result.response.string("question"))
        assertTrue(result.response.string("model_instruction").contains("clean current frame"))
        assertTrue(result.response.getValue("frame").jsonObject.boolean("clean"))
        assertEquals(1, backend.lookCalls)
    }

    @Test
    fun `visual failures keep secure rate display and stale causes typed`() = runTest {
        val cases = listOf(
            failure("screenshot_secure_window", retryable = false) to "degraded",
            failure("screenshot_rate_limited", retryable = true) to "degraded",
            failure("unsupported_display", retryable = false) to "unsupported",
            failure("stale_frame", retryable = true, fresh = true) to "degraded",
        )
        cases.forEach { (failure, state) ->
            val backend = FakeVisualBackend().apply { reset = failure }
            val result = VisualToolHandlers(backend).resetView(JOB)

            assertEquals(failure.code, result.response.string("code"))
            assertEquals(state, result.response.string("provider_state"))
            assertEquals("proven_no_effect", result.response.string("effect_status"))
            assertNull(result.screenFramePayload)
            assertFalse(result.mutating)
        }
    }

    @Test
    fun `look validates its question before capture`() = runTest {
        val backend = FakeVisualBackend()

        val result = VisualToolHandlers(backend).look(JOB, JsonObject(emptyMap()))

        assertEquals("invalid_arguments", result.response.string("code"))
        assertEquals(0, backend.lookCalls)
    }

    private class FakeVisualBackend : VisualToolBackend {
        override val observationGeneration: Long = 17L
        var reset: VisualProviderResult<VisualFrame> = VisualProviderResult.Success(frame())
        var whole: VisualProviderResult<VisualFrame> = VisualProviderResult.Success(frame())
        var zoomed: VisualProviderResult<VisualFrame> = VisualProviderResult.Success(frame())
        var looked: VisualProviderResult<VisualFrame> = VisualProviderResult.Success(frame(clean = true))
        val zoomCells = mutableListOf<Int>()
        var lookCalls = 0

        override suspend fun resetView() = reset

        override suspend fun seeWholeScreen() = whole

        override suspend fun zoom(cell: Int): VisualProviderResult<VisualFrame> {
            zoomCells += cell
            return zoomed
        }

        override suspend fun look(): VisualProviderResult<VisualFrame> {
            lookCalls += 1
            return looked
        }
    }

    private companion object {
        val JOB = PhoneControlToolJobContext(4, "visual-job", 9)

        fun grid(
            bounds: TargetBounds = TargetBounds(0, 0, 600, 1_000),
            windowId: Long? = 8,
            packageOrSurface: String = "dev.visual",
        ) = VisualGridIdentity(
            observationGeneration = 17,
            visualRevision = 23,
            displayId = 0,
            windowId = windowId,
            packageOrSurface = packageOrSurface,
            bounds = bounds,
            rotation = 0,
            densityDpi = 320,
            capturedAtMs = 44,
        )

        fun frame(
            kind: VisualViewKind = VisualViewKind.ACTIVE_SURFACE,
            payload: String = "frame-payload",
            clean: Boolean = false,
            windowId: Long? = 8,
        ): VisualFrame {
            val bounds = TargetBounds(0, 0, 600, 1_000)
            return VisualFrame(
                identity = VisualFrameIdentity(
                    observationGeneration = 17,
                    visualRevision = 23,
                    displayId = 0,
                    windowId = windowId,
                    packageOrSurface = if (windowId == null) "android-display-0" else "dev.visual",
                    cropBounds = bounds,
                    captureWidth = 600,
                    captureHeight = 1_000,
                    rotation = 0,
                    densityDpi = 320,
                    capturedAtMs = 44,
                    viewKind = kind,
                    clean = clean,
                    grid = if (clean) null else grid(bounds, windowId),
                ),
                screenPayload = payload,
            )
        }

        fun failure(
            code: String,
            retryable: Boolean,
            fresh: Boolean = false,
        ) = VisualProviderResult.Failure(
            code = code,
            message = code,
            retryable = retryable,
            freshObservationRequired = fresh,
        )

        fun args(vararg values: Pair<String, Any>): JsonObject = buildJsonObject {
            values.forEach { (key, value) ->
                when (value) {
                    is Int -> put(key, value)
                    is String -> put(key, value)
                    else -> error("unsupported test argument")
                }
            }
        }

        fun JsonObject.string(name: String): String =
            getValue(name).jsonPrimitive.contentOrNull ?: error("missing $name")

        fun JsonObject.long(name: String): Long = getValue(name).jsonPrimitive.content.toLong()

        fun JsonObject.boolean(name: String): Boolean = getValue(name).jsonPrimitive.content.toBoolean()
    }
}

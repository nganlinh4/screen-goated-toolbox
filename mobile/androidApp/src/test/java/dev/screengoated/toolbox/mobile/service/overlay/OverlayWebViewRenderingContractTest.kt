package dev.screengoated.toolbox.mobile.service.overlay

import android.view.WindowManager
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class OverlayWebViewRenderingContractTest {
    @Test
    fun windowFlagsEnableHardwareAccelerationWithoutDroppingFeatureFlags() {
        val base = WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN

        val actual = overlayWebViewWindowFlags(base)

        assertEquals(base, actual and base)
        assertTrue(actual and WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED != 0)
    }

    @Test
    fun everyOverlayConsumerUsesTheSharedDirectCompositionPolicy() {
        val fixture = Json.parseToJsonElement(repoFile(FIXTURE_PATH).readText()).jsonObject
        val consumers = fixture.getValue("consumers").jsonArray
            .map { it.jsonPrimitive.content }
        val invariants = fixture.getValue("invariants").jsonObject

        assertEquals(
            listOf("phone_control_orb", "live_translate_pane", "preset_overlay_window"),
            consumers,
        )
        assertEquals("window_before_attach", invariants.string("hardwareAccelerationOwner"))
        assertEquals("none_direct_window_composition", invariants.string("webViewLayer"))
        assertEquals(
            "at_most_once_per_display_frame",
            invariants.string("latestVisualDispatchCadence"),
        )
        assertEquals(
            "at_most_once_per_display_frame",
            invariants.string("gestureGeometryCadence"),
        )
        assertEquals(
            "after_committed_terminal_geometry",
            invariants.string("gestureControlReveal"),
        )
        assertEquals(
            "gesture_terminal_only",
            invariants.string("phoneControlPositionPersistence"),
        )
        assertEquals(
            "platform_result_renderer_not_nested_native_surface",
            invariants.string("authoredHtmlSurfaceOwner"),
        )
        assertEquals(
            "result_overlay_exact_rounded_bounds",
            invariants.string("authoredHtmlClip"),
        )
        assertEquals(
            "same_frame_as_result_overlay",
            invariants.string("authoredHtmlGestureGeometry"),
        )
        assertEquals(
            "platform_native_browser_surface",
            invariants.string("externalNavigationSurfaceOwner"),
        )
        assertEquals(
            "same_frame_as_result_overlay",
            invariants.string("externalNavigationGestureGeometry"),
        )
        assertEquals(
            "result_geometry_not_page_content",
            invariants.string("externalNavigationResizeHitTestOwner"),
        )
        assertEquals(
            "platform_antialiased_rounded_bounds",
            invariants.string("externalNavigationClip"),
        )
        assertEquals(
            "fully_opaque_while_visible",
            invariants.string("externalNavigationOpacity"),
        )
        assertEquals(
            "result_processing_outline_until_page_finished",
            invariants.string("externalNavigationLoadingHandoff"),
        )
    }

    private fun repoFile(path: String): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.map { root -> File(root, path) }
            .firstOrNull(File::isFile)
            ?: error("Could not locate $path from $workingDirectory")
    }

    private fun kotlinx.serialization.json.JsonObject.string(field: String): String =
        getValue(field).jsonPrimitive.content

    private companion object {
        const val FIXTURE_PATH =
            "parity-fixtures/android-webview-overlays/rendering-contract.json"
    }
}

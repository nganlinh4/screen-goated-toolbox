package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class AccessibilityTraversalTest {
    @Test
    fun api30AndLaterFlattenEveryDisplayUsingTheDisplayMapKey() {
        listOf(30, 36).forEach { apiLevel ->
            val selected = selectAccessibilityWindows(
                apiLevel = apiLevel,
                defaultWindows = { error("default-display fallback must not run") },
                allDisplayWindows = {
                    listOf(
                        AccessibilityDisplayWindows(displayId = 3, windows = listOf("left", "right")),
                        AccessibilityDisplayWindows(displayId = 11, windows = listOf("presentation")),
                    )
                },
            )

            assertEquals(
                listOf(3 to "left", 3 to "right", 11 to "presentation"),
                selected.map { window -> window.displayId to window.window },
            )
        }
    }

    @Test
    fun api29UsesOnlyTheDefaultDisplayFallback() {
        var allDisplayRead = false
        val selected = selectAccessibilityWindows(
            apiLevel = 29,
            defaultWindows = { listOf("first", "second") },
            allDisplayWindows = {
                allDisplayRead = true
                listOf(AccessibilityDisplayWindows(displayId = 8, windows = listOf("wrong")))
            },
        )

        assertFalse(allDisplayRead)
        assertEquals(listOf(0 to "first", 0 to "second"), selected.map { it.displayId to it.window })
    }

    @Test
    fun invalidDisplayKeysAreRejectedInsteadOfCoercedToTheDefaultDisplay() {
        assertThrows(IllegalArgumentException::class.java) {
            AccessibilityDisplayWindows(displayId = -1, windows = listOf("window"))
        }
    }

    @Test
    fun platformBoundsRejectInvertedEdgesWithoutWeakeningTheDomainInvariant() {
        assertEquals(TargetBounds(-20, 10, 30, 40), validPlatformTargetBounds(-20, 10, 30, 40))
        assertEquals(TargetBounds(4, 7, 4, 7), validPlatformTargetBounds(4, 7, 4, 7))
        assertEquals(null, validPlatformTargetBounds(30, 10, 20, 40))
        assertEquals(null, validPlatformTargetBounds(20, 40, 30, 10))
    }

    @Test
    fun activeRootFallbackRequiresTheExactActiveOrFocusedWindow() {
        val ids = mapOf("listed" to 4, "active" to 9)
        val rootId: (String) -> Int = { root -> ids.getValue(root) }

        assertEquals(
            "listed",
            selectAccessibilityWindowRoot("listed", "active", 9, true, false, rootId),
        )
        assertEquals(
            "active",
            selectAccessibilityWindowRoot(null, "active", 9, true, false, rootId),
        )
        assertEquals(null, selectAccessibilityWindowRoot(null, "active", 8, true, false, rootId))
        assertEquals(null, selectAccessibilityWindowRoot(null, "active", 9, false, false, rootId))
    }

    @Test
    fun missingActiveRootBecomesObservableWithoutClaimingApplicationAuthority() {
        val listed = listOf(
            capturedWindow(
                id = 4,
                title = "System surface",
                packageName = "system.package",
                root = "system",
                overlay = false,
                pictureInPicture = false,
            ),
        )

        val supplemented = supplementMissingActiveRoot(
            listed,
            ActiveAccessibilityRoot(
                displayId = 0,
                id = 9,
                packageName = "content.package",
                bounds = TargetBounds(0, 0, 1200, 800),
                root = "content",
            ),
        )

        assertEquals(2, supplemented.size)
        with(supplemented.last()) {
            assertEquals(9, id)
            assertEquals("active_content", type)
            assertEquals("content.package", packageName)
            assertTrue(active)
            assertFalse(focused)
            assertEquals("content", root)
            assertTrue(layer > listed.single().layer)
        }
    }

    @Test
    fun listedWindowIdentityPreventsDuplicateActiveRootSurface() {
        val listed = listOf(
            capturedWindow(
                id = 9,
                title = "Authoritative window",
                packageName = "content.package",
                root = "listed",
                overlay = false,
                pictureInPicture = false,
            ),
        )

        val supplemented = supplementMissingActiveRoot(
            listed,
            ActiveAccessibilityRoot(
                displayId = 7,
                id = 9,
                packageName = "content.package",
                bounds = TargetBounds(0, 0, 1200, 800),
                root = "fallback",
            ),
        )

        assertEquals(listed, supplemented)
    }

    @Test
    fun sameWindowIdOnAnotherDisplayDoesNotHideTheActiveRoot() {
        val listed = listOf(
            capturedWindow(
                id = 9,
                title = "Presentation",
                packageName = "content.package",
                root = "presentation",
                overlay = false,
                pictureInPicture = false,
            ),
        )
        val supplemented = supplementMissingActiveRoot(
            listed,
            ActiveAccessibilityRoot(
                displayId = 0,
                id = 9,
                packageName = "content.package",
                bounds = TargetBounds(0, 0, 1200, 800),
                root = "active",
            ),
        )

        assertEquals(listOf(7, 0), supplemented.map { window -> window.displayId })
    }

    @Test
    fun metadataLessRootRequiresAnUnambiguousDisplay() {
        val root = TargetBounds(0, 0, 1200, 800)
        assertEquals(
            4,
            resolveActiveRootDisplay(
                root,
                listOf(
                    AccessibilityDisplayExtent(4, root),
                    AccessibilityDisplayExtent(9, TargetBounds(0, 0, 2560, 1600)),
                ),
            ),
        )
        assertEquals(
            null,
            resolveActiveRootDisplay(
                TargetBounds(10, 10, 100, 100),
                listOf(
                    AccessibilityDisplayExtent(4, TargetBounds(0, 0, 1200, 800)),
                    AccessibilityDisplayExtent(9, TargetBounds(0, 0, 2560, 1600)),
                ),
            ),
        )
    }

    @Test
    fun authoritativeActiveWindowIsAppendedByExactDisplayAndWindowPair() {
        val listed = listOf(AccessibilityWindowOnDisplay(0, "listed"))
        val ids = mapOf("listed" to 4, "candidate" to 9)
        val appended = appendMissingAccessibilityWindow(
            listed,
            AccessibilityWindowOnDisplay(7, "candidate"),
            ids::getValue,
        )
        assertEquals(listOf(0 to "listed", 7 to "candidate"), appended.map { it.displayId to it.window })
        assertEquals(
            appended,
            appendMissingAccessibilityWindow(
                appended,
                AccessibilityWindowOnDisplay(7, "candidate"),
                ids::getValue,
            ),
        )
    }

    @Test
    fun snapshotsRetainRootlessWindowsAndPublishStructuralMetadata() {
        val snapshots = snapshotAccessibilityWindows(
            windows = listOf(
                capturedWindow(
                    id = 41,
                    title = "Bảo mật 한글",
                    packageName = null,
                    root = null,
                    overlay = false,
                    pictureInPicture = false,
                ),
                capturedWindow(
                    id = 42,
                    title = "Controller overlay",
                    packageName = null,
                    root = "node",
                    overlay = true,
                    pictureInPicture = false,
                ),
                capturedWindow(
                    id = 43,
                    title = "Controller activity",
                    packageName = "controller.package",
                    root = "node",
                    overlay = false,
                    pictureInPicture = true,
                ),
                capturedWindow(
                    id = 44,
                    title = "Controller fallback overlay",
                    packageName = "controller.package",
                    root = "node",
                    overlay = false,
                    pictureInPicture = false,
                    type = "system",
                ),
            ),
            servicePackage = "controller.package",
        )

        assertEquals(4, snapshots.size)
        with(snapshots[0]) {
            assertEquals("Bảo mật 한글", title)
            assertFalse(contentAccessible)
            assertFalse(controllerOwned)
            assertFalse(pictureInPicture)
        }
        with(snapshots[1]) {
            assertTrue(contentAccessible)
            assertTrue(controllerOwned)
            assertFalse(pictureInPicture)
        }
        with(snapshots[2]) {
            assertTrue(contentAccessible)
            assertFalse(controllerOwned)
            assertTrue(pictureInPicture)
        }
        with(snapshots[3]) {
            assertTrue(contentAccessible)
            assertTrue(controllerOwned)
            assertFalse(pictureInPicture)
        }
    }

    private fun capturedWindow(
        id: Int,
        title: String,
        packageName: String?,
        root: String?,
        overlay: Boolean,
        pictureInPicture: Boolean,
        type: String = "application",
    ): CapturedAccessibilityWindow<String> = CapturedAccessibilityWindow(
        displayId = 7,
        id = id,
        layer = 2,
        type = type,
        title = title,
        packageName = packageName,
        active = true,
        focused = true,
        bounds = TargetBounds(0, 0, 100, 200),
        accessibilityOverlay = overlay,
        pictureInPicture = pictureInPicture,
        root = root,
    )
}

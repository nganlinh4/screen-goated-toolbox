package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class AndroidSurfaceIdentityTest {
    @Test
    fun stableTargetRoundTripsEveryIdentityField() {
        val identity = identity(generation = 42, displayId = 3, windowId = 987, packageName = "dev.example.app")

        val parsed = AndroidSurfaceIdentity.parseStableTarget(identity.stableTarget())

        assertEquals(AndroidSurfaceTargetParseResult.Stable(identity), parsed)
        assertEquals("@android-window:v1:42:3:987:dev.example.app", identity.stableTarget())
    }

    @Test
    fun blankPackageStillRoundTripsForSemanticallyBlindWindows() {
        val identity = identity(packageName = "")

        assertEquals(
            AndroidSurfaceTargetParseResult.Stable(identity),
            AndroidSurfaceIdentity.parseStableTarget(identity.stableTarget()),
        )
        assertTrue(identity.stableTarget().endsWith(":"))
    }

    @Test
    fun malformedStableTargetsNeverFallThroughToHumanMatching() {
        val malformed = listOf(
            "@android-window:",
            "@android-window:v2:4:0:7:dev.example",
            "@android-window:v1:4:0:7",
            "@android-window:v1:0:0:7:dev.example",
            "@android-window:v1:04:0:7:dev.example",
            "@android-window:v1:4:-1:7:dev.example",
            "@android-window:v1:4:0:-1:dev.example",
            "@android-window:v1:4:0:07:dev.example",
            "@android-window:v1:4:0:7:dev:example",
            "@android-window:v1:4:0:7:dev example",
        )

        malformed.forEach { target ->
            val parsed = AndroidSurfaceIdentity.parseStableTarget(target)
            assertTrue("expected malformed: $target", parsed is AndroidSurfaceTargetParseResult.Malformed)
            val resolved = lease().resolve(target)
            assertRejected<AndroidSurfaceResolutionError.MalformedStableTarget>(resolved)
        }
    }

    @Test
    fun stableResolutionRejectsOldObservationBeforeInspectingReusedIds() {
        val current = descriptor(generation = 8, displayId = 0, windowId = 4, packageName = "dev.current")
        val target = identity(generation = 7, displayId = 0, windowId = 4, packageName = "dev.current")

        val error = assertRejected<AndroidSurfaceResolutionError.StaleGeneration>(
            AndroidSurfaceLease(8, listOf(current)).resolve(target.stableTarget()),
        )

        assertEquals(7, error.targetGeneration)
        assertEquals(8, error.currentGeneration)
    }

    @Test
    fun stableResolutionRejectsWindowIdReusedByAnotherPackage() {
        val current = descriptor(displayId = 1, windowId = 9, packageName = "dev.current")
        val target = identity(displayId = 1, windowId = 9, packageName = "dev.previous")

        val error = assertRejected<AndroidSurfaceResolutionError.WrongPackage>(
            lease(current).resolve(target.stableTarget()),
        )

        assertEquals("dev.previous", error.expectedPackage)
        assertEquals(listOf("dev.current"), error.currentPackages)
    }

    @Test
    fun stableResolutionRejectsSameWindowAndPackageOnAnotherDisplay() {
        val current = descriptor(displayId = 2, windowId = 9, packageName = "dev.example")
        val target = identity(displayId = 1, windowId = 9, packageName = "dev.example")

        val error = assertRejected<AndroidSurfaceResolutionError.WrongDisplay>(
            lease(current).resolve(target.stableTarget()),
        )

        assertEquals(1, error.expectedDisplay)
        assertEquals(listOf(2), error.currentDisplays)
    }

    @Test
    fun stableResolutionRejectsWindowIdReusedAcrossBothPackageAndDisplay() {
        val current = descriptor(displayId = 2, windowId = 9, packageName = "dev.current")
        val target = identity(displayId = 1, windowId = 9, packageName = "dev.previous")

        val error = assertRejected<AndroidSurfaceResolutionError.ReusedWindowId>(
            lease(current).resolve(target.stableTarget()),
        )

        assertEquals(9, error.windowId)
        assertEquals(listOf(current.target), error.currentTargets)
    }

    @Test
    fun exactStableIdentityResolvesOnlyItsDescriptor() {
        val first = descriptor(displayId = 0, windowId = 3, packageName = "dev.first")
        val second = descriptor(displayId = 1, windowId = 3, packageName = "dev.second")

        val resolution = lease(first, second).resolve(second.target)

        assertEquals(AndroidSurfaceResolution.Resolved(second), resolution)
    }

    @Test
    fun humanTargetsUseNfkcRootCaseFoldingAndUnicodeWhitespace() {
        val previousLocale = Locale.getDefault()
        Locale.setDefault(Locale.forLanguageTag("tr-TR"))
        try {
            val surface = descriptor(
                title = "  ＭＹ\u2003INTERACTIVE\tWINDOW  ",
                packageName = "DEV.EXAMPLE.APP",
            )
            assertEquals(
                AndroidSurfaceResolution.Resolved(surface),
                lease(surface).resolve("my interactive window"),
            )
            assertEquals(
                AndroidSurfaceResolution.Resolved(surface),
                lease(surface).resolve("ｄｅｖ．ｅｘａｍｐｌｅ．ａｐｐ"),
            )
        } finally {
            Locale.setDefault(previousLocale)
        }
    }

    @Test
    fun humanResolutionIsExactAndDoesNotUseSubstrings() {
        val surface = descriptor(title = "Document 10")

        val error = assertRejected<AndroidSurfaceResolutionError.NamedTargetNotFound>(
            lease(surface).resolve("Document 1"),
        )

        assertEquals("Document 1", error.target)
    }

    @Test
    fun ambiguousHumanTargetReturnsDeterministicallySortedStableChoices() {
        val first = descriptor(displayId = 2, windowId = 8, packageName = "dev.z", title = "Notes")
        val second = descriptor(displayId = 0, windowId = 2, packageName = "dev.a", title = " notes ")

        val forward = assertRejected<AndroidSurfaceResolutionError.Ambiguous>(
            lease(first, second).resolve("ＮＯＴＥＳ"),
        )
        val reverse = assertRejected<AndroidSurfaceResolutionError.Ambiguous>(
            lease(second, first).resolve("notes"),
        )

        assertEquals(listOf(second.target, first.target).sorted(), forward.choices)
        assertEquals(forward.choices, reverse.choices)
    }

    @Test
    fun emptyHumanTargetIsRejectedAndLeaseDefensivelyCopiesItsInput() {
        val mutable = mutableListOf(descriptor())
        val lease = AndroidSurfaceLease(GENERATION, mutable)
        mutable.clear()

        assertEquals(1, lease.surfaces.size)
        assertRejected<AndroidSurfaceResolutionError.EmptyTarget>(lease.resolve(" \u2003\t "))
    }

    @Test
    fun leaseRejectsMixedGenerationsAndDuplicateIdentities() {
        expectIllegalArgument {
            AndroidSurfaceLease(GENERATION, listOf(descriptor(generation = GENERATION + 1)))
        }
        val duplicate = descriptor()
        expectIllegalArgument {
            AndroidSurfaceLease(GENERATION, listOf(duplicate, duplicate.copy(title = "Other")))
        }
    }

    private inline fun <reified T : AndroidSurfaceResolutionError> assertRejected(
        resolution: AndroidSurfaceResolution,
    ): T {
        val rejected = when (resolution) {
            is AndroidSurfaceResolution.Rejected -> resolution
            is AndroidSurfaceResolution.Resolved -> throw AssertionError(
                "expected rejected resolution, got $resolution",
            )
        }
        assertTrue("expected ${T::class.java.simpleName}, got ${rejected.error}", rejected.error is T)
        return rejected.error as T
    }

    private fun expectIllegalArgument(block: () -> Unit) {
        try {
            block()
            fail("expected IllegalArgumentException")
        } catch (_: IllegalArgumentException) {
            // Expected structural rejection.
        }
    }

    private fun lease(vararg surfaces: AndroidSurfaceDescriptor): AndroidSurfaceLease =
        AndroidSurfaceLease(GENERATION, surfaces.toList())

    private fun descriptor(
        generation: Long = GENERATION,
        displayId: Int = 0,
        windowId: Long = 1,
        packageName: String = "dev.example",
        title: String? = "Example",
    ) = AndroidSurfaceDescriptor(
        identity = identity(generation, displayId, windowId, packageName),
        title = title,
        active = false,
        focused = false,
    )

    private fun identity(
        generation: Long = GENERATION,
        displayId: Int = 0,
        windowId: Long = 1,
        packageName: String = "dev.example",
    ) = AndroidSurfaceIdentity(generation, displayId, windowId, packageName)

    private companion object {
        const val GENERATION = 11L
    }
}

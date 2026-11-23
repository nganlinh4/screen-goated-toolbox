package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class AppsCarouselDecorationDrawPhaseTest {
    @Test
    fun springValuesAreReadInTheDrawScopeNotDuringComposition() {
        val source = File(repoRoot(), DECORATION_PATH).readText()
        // A delegated `by animateFloatAsState` read recomposes the tile on every spring frame.
        assertFalse(source.contains("by animateFloatAsState("))
        val canvas = source.substringAfter("Canvas(modifier = modifier.fillMaxSize()) {")
        assertTrue(canvas.contains("anim.morphProgress()"))
        assertTrue(canvas.contains("anim.rotation()"))
    }

    @Test
    fun offScreenWarmPagesScheduleNoMorphs() {
        assertTrue(shellPageActive(page = 1, settledPage = 1))
        assertFalse(shellPageActive(page = 0, settledPage = 1))
        val source = File(repoRoot(), DECORATION_PATH).readText()
        val loop = source.substringAfter("LaunchedEffect(isScrolling, i, pageActive) {").substringBefore("while (true)")
        assertTrue(loop.contains("if (!pageActive) return@LaunchedEffect"))
        assertTrue(source.contains("val pageActive = LocalShellPageActive.current"))
        val surface = File(repoRoot(), "$UI_DIR/MobileShellSurface.kt").readText()
        assertTrue(surface.contains("LocalShellPageActive provides pageActive"))
        assertTrue(surface.contains("shellPageActive(page, pagerState.settledPage)"))
    }

    @Test
    fun warmShellPagesReceiveOnlyTheRunningFlagNotTheWholeSessionState() {
        val running = setOf(SessionPhase.STARTING, SessionPhase.LISTENING, SessionPhase.TRANSLATING)
        SessionPhase.entries.forEach { phase -> assertEquals(phase in running, phase.isLiveSessionRunning()) }
        listOf("MobileShellSections.kt", "AppsCarouselSection.kt", "AppsCarouselTiles.kt").forEach { name ->
            assertFalse(name, File(repoRoot(), "$UI_DIR/$name").readText().contains("LiveSessionState"))
        }
    }

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current -> current.parentFile }
            .firstOrNull { File(it, DECORATION_PATH).exists() }
            ?: error("Could not locate $DECORATION_PATH from $workingDirectory")
    }

    private companion object {
        const val UI_DIR = "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui"
        const val DECORATION_PATH = "$UI_DIR/AppsCarouselDecoration.kt"
    }
}

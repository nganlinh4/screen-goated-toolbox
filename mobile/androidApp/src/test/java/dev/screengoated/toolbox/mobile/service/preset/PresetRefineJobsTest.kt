package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import java.io.File
import kotlinx.coroutines.Job
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetRefineJobsTest {
    private val window = PresetResultWindowId(sessionId = "session", blockIdx = 0)
    private val other = PresetResultWindowId(sessionId = "session", blockIdx = 1)

    @Test
    fun secondRefineCancelsTheFirstAndSilencesItsCallbacks() {
        val jobs = PresetRefineJobs()
        val first = Job()
        val firstToken = jobs.begin(window)
        jobs.attach(window, firstToken, first)

        val second = Job()
        val secondToken = jobs.begin(window)
        jobs.attach(window, secondToken, second)

        assertTrue(first.isCancelled)
        assertFalse(jobs.isCurrent(window, firstToken))
        assertFalse(jobs.finish(window, firstToken))
        assertTrue(jobs.isCurrent(window, secondToken))
        assertFalse(second.isCancelled)
        assertTrue(jobs.finish(window, secondToken))
        assertFalse(jobs.isCurrent(window, secondToken))
    }

    @Test
    fun closeCancelsOnlyThatWindowAndDismissAllCancelsEverything() {
        val jobs = PresetRefineJobs()
        val a = Job().also { jobs.attach(window, jobs.begin(window), it) }
        val bToken = jobs.begin(other)
        val b = Job().also { jobs.attach(other, bToken, it) }

        jobs.cancel(window)
        assertTrue(a.isCancelled)
        assertFalse(b.isCancelled)
        assertTrue(jobs.isCurrent(other, bToken))

        jobs.cancelAll()
        assertTrue(b.isCancelled)
        assertFalse(jobs.isCurrent(other, bToken))
    }

    @Test
    fun refineThatFinishedBeforeAttachIsNotRetained() {
        val jobs = PresetRefineJobs()
        val token = jobs.begin(window)
        assertTrue(jobs.finish(window, token))
        val late = Job()
        jobs.attach(window, token, late)
        assertTrue(late.isCancelled)
    }

    @Test
    fun standaloneWindowsCannotRefine() {
        assertFalse(refineSupported("${STANDALONE_RESULT_PRESET_PREFIX}help"))
        assertTrue(refineSupported("preset_translate"))
    }

    @Test
    fun everyWindowRemovalCancelsItsRefine() {
        val root = repoRoot()
        val base = "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset"
        val lifecycle = File(root, "$base/PresetOverlayResultLifecycleSupport.kt").readText()
        val module = File(root, "$base/PresetOverlayResultModule.kt").readText()
        val canvas = File(root, "$base/PresetOverlayResultCanvasSupport.kt").readText()

        assertTrue(
            "every remove is preceded by a refine cancel",
            Regex("""refineJobs\.cancel\(id\)\s+(val removed = )?resultWindows\.remove\(id\)""")
                .findAll(lifecycle).count() == Regex("""resultWindows\.remove\(id\)""").findAll(lifecycle).count(),
        )
        assertTrue(module.substringAfter("fun destroy() {").trimStart().startsWith("refineJobs.cancelAll()"))
        val submit = canvas.substringAfter("\"submit_refine\" ->").substringBefore("\"cancel_refine\" ->")
        assertTrue(submit.indexOf("refineSupported(") < submit.indexOf("textHistory = runtime.textHistory + currentText"))
        assertTrue(submit.contains("refineJobs.attach(windowId, refineToken, refineJob)"))
    }

    private fun repoRoot(): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return directory
    }
}

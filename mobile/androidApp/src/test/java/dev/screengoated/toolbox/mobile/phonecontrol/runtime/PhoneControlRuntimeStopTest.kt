package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlRuntimeStopTest {
    @Test
    fun stop_doesNotTouchCoordinatorOnCallerThread() {
        val executor = Executors.newSingleThreadExecutor { runnable -> Thread(runnable, LOOP_THREAD) }
        val scope = CoroutineScope(SupervisorJob() + executor.asCoroutineDispatcher())
        val loopRunning = CountDownLatch(1)
        val released = CountDownLatch(1)
        val loopExited = AtomicBoolean(false)
        val loopThread = AtomicReference<Thread>()
        val releaseThread = AtomicReference<Thread>()
        val loopExitedAtRelease = AtomicBoolean(false)
        try {
            val job = scope.launch {
                try {
                    loopThread.set(Thread.currentThread())
                    loopRunning.countDown()
                    while (true) delay(5)
                } finally {
                    loopExited.set(true)
                }
            }
            assertTrue(loopRunning.await(5, TimeUnit.SECONDS))

            stopAfterSessionJob(job) {
                releaseThread.set(Thread.currentThread())
                loopExitedAtRelease.set(loopExited.get())
                released.countDown()
            }

            assertTrue(released.await(5, TimeUnit.SECONDS))
            // Compare threads, not names: coroutine debug mode renames a thread while a coroutine runs.
            assertSame(loopThread.get(), releaseThread.get())
            assertTrue("teardown must wait for the transport loop to unwind", loopExitedAtRelease.get())
        } finally {
            scope.cancel()
            executor.shutdown()
        }
    }

    @Test
    fun jobThatNeverRan_stillReleases() {
        val scope = CoroutineScope(SupervisorJob())
        var releases = 0
        try {
            val job = scope.launch(start = CoroutineStart.LAZY) { error("never runs") }
            stopAfterSessionJob(job) { releases += 1 }
            assertEquals(1, releases)
        } finally {
            scope.cancel()
        }
    }

    @Test
    fun noJob_releasesImmediately() {
        var releases = 0
        stopAfterSessionJob(null) { releases += 1 }
        assertEquals(1, releases)
    }

    @Test
    fun runtimeStop_neverReleasesBesideTheLoop() {
        val source = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/runtime/PhoneControlRuntime.kt",
        ).readText()
        val stop = source.substringAfter("    fun stop() {").substringBefore("\n    }\n")
        assertTrue(stop.contains("stopAfterSessionJob(job, ::releaseResources)"))
        assertFalse(stop.contains("\n        releaseResources()"))
    }

    private fun repoFile(path: String): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        return File(directory, path)
    }

    private companion object {
        const val LOOP_THREAD = "phone-control-loop"
    }
}

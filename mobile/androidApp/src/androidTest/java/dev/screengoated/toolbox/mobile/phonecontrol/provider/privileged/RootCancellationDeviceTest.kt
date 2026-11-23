package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Requires an explicitly authorized root manager on a disposable device. */
class RootCancellationDeviceTest {
    @Test
    fun timeoutAndCancellationRetireTheRootShellAndItsChild() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val version = root("su -v").trim()
        assertTrue("Expected the real Magisk su client: $version", version.contains("MAGISKSU"))
        val evidence = File(context.cacheDir, "root-cancellation-live.txt")
        evidence.writeText("su=$version\n")
        for (cancel in listOf(false, true)) {
            val id = "live-root-${System.nanoTime()}"
            val began = System.nanoTime()
            val executor = Executors.newSingleThreadExecutor()
            try {
                val future = executor.submit<JsonObject> {
                    rootBoundedProcessRunner.run(
                        operationId = id,
                        command = listOf(SHELL_PROGRAM, "-c", "sleep 90 & child=\$!; echo PIDS \$\$ \$child; wait"),
                        cwd = null,
                        timeoutMs = if (cancel) 30_000 else 2_000,
                        authorityUid = 0,
                    )
                }
                if (cancel) {
                    Thread.sleep(1_000)
                    rootBoundedProcessRunner.requestCancellation(id)
                }
                val receipt = future.get(20, TimeUnit.SECONDS)
                val elapsed = TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - began)
                evidence.appendText("cancel=$cancel elapsed_ms=$elapsed receipt=$receipt\n")
                assertEquals(receipt.toString(), !cancel, receipt["timed_out"]?.jsonPrimitive?.booleanOrNull)
                assertEquals(receipt.toString(), cancel, receipt["cancelled"]?.jsonPrimitive?.booleanOrNull)
                if (cancel) assertTrue(receipt["terminal_cancellation_acknowledged"]?.jsonPrimitive?.booleanOrNull == true)
                val pids = Regex("PIDS (\\d+) (\\d+)").find(receipt["output"]!!.jsonPrimitive.content)
                assertTrue("Root shell did not start: $receipt", pids != null)
                val ps = root("ps -A -o PID,PPID,ARGS")
                evidence.appendText("postcondition_ps_A:\n$ps\n")
                val livePids = ps.lineSequence().drop(1).mapNotNull { it.trim().split(Regex("\\s+"), limit = 2).firstOrNull()?.toIntOrNull() }.toSet()
                for (pid in requireNotNull(pids).groupValues.drop(1)) {
                    assertFalse("Root child $pid survived:\n$ps", pid.toInt() in livePids)
                }
                assertTrue("Termination exceeded its budget: $elapsed", elapsed < 15_000)
            } finally {
                rootBoundedProcessRunner.requestCancellation(id)
                executor.shutdownNow()
            }
        }
    }

    private fun root(command: String): String {
        val process = ProcessBuilder("su", "-c", command).redirectErrorStream(true).start()
        assertTrue("Root helper timed out", process.waitFor(10, TimeUnit.SECONDS))
        val text = process.inputStream.bufferedReader().readText()
        assertEquals(text, 0, process.exitValue())
        return text
    }
}

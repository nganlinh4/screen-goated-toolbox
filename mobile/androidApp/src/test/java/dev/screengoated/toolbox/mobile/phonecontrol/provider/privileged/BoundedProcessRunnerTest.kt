package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BoundedProcessRunnerTest {
    @Test
    fun cancellationBeforeDispatchNeverLaunchesProcess() {
        val launches = AtomicInteger(0)
        val runner = BoundedProcessRunner { _, _ ->
            launches.incrementAndGet()
            BoundedLaunchedProcess(FakeProcess())
        }

        runner.requestCancellation("operation-before")
        val receipt = runner.run("operation-before", command(), null, 1_000L, 2000)

        assertEquals(0, launches.get())
        assertEquals("process_cancelled", receipt.string("code"))
        assertFalse(receipt.boolean("process_started"))
        assertTrue(receipt.boolean("terminal_cancellation_acknowledged"))
    }

    @Test
    fun cancellationAfterStartDestroysAndWaitsForExactProcess() {
        val process = FakeProcess()
        val runner = BoundedProcessRunner { _, _ ->
            BoundedLaunchedProcess(process.also { it.launched.countDown() })
        }
        val result = AsyncReceipt {
            runner.run("operation-running", command(), null, 5_000L, 2000)
        }
        assertTrue(process.launched.await(WAIT_MS, TimeUnit.MILLISECONDS))

        val request = runner.requestCancellation("operation-running")
        val receipt = result.await()

        assertEquals("cancellation_requested", request.string("code"))
        assertTrue(process.destroyRequested.get())
        assertEquals("process_cancelled", receipt.string("code"))
        assertTrue(receipt.boolean("terminal_cancellation_acknowledged"))
    }

    @Test
    fun timeoutCancellationRaceCannotAcknowledgeLivingProcess() {
        val process = FakeProcess(ignoreDestroy = true)
        val runner = BoundedProcessRunner { _, _ ->
            BoundedLaunchedProcess(process.also { it.launched.countDown() })
        }
        val result = AsyncReceipt {
            runner.run("operation-stubborn", command(), null, 100L, 2000)
        }
        assertTrue(process.launched.await(WAIT_MS, TimeUnit.MILLISECONDS))

        runner.requestCancellation("operation-stubborn")
        assertFalse("a living process cannot produce a terminal receipt", result.completed.await(250L, TimeUnit.MILLISECONDS))

        process.complete(143)
        val receipt = result.await()
        assertEquals("process_cancelled", receipt.string("code"))
        assertTrue(receipt.boolean("terminal_cancellation_acknowledged"))
    }

    @Test
    fun cancellationWaitsForTheOwnedProcessGroupAfterTheLeaderExits() {
        val process = FakeProcess(ignoreDestroy = true)
        val groupState = AtomicReference(ProcessGroupState.ALIVE)
        val signalled = ConcurrentHashMap.newKeySet<Int>()
        val groupFile = File.createTempFile("phone-control-process-group-", ".pid").apply {
            writeText("4242")
        }
        val controller = object : ProcessGroupController {
            override fun signal(groupId: Int, signal: Int, authorityUid: Int) {
                assertEquals(4242, groupId)
                signalled += signal
            }

            override fun state(groupId: Int, authorityUid: Int): ProcessGroupState =
                groupState.get()
        }
        val runner = BoundedProcessRunner(groupController = controller) { _, _ ->
            BoundedLaunchedProcess(process.also { it.launched.countDown() }, groupFile)
        }
        val result = AsyncReceipt {
            runner.run("operation-tree", command(), null, 5_000L, 2000)
        }
        assertTrue(process.launched.await(WAIT_MS, TimeUnit.MILLISECONDS))

        runner.requestCancellation("operation-tree")
        process.complete(143)
        assertFalse(result.completed.await(250L, TimeUnit.MILLISECONDS))

        groupState.set(ProcessGroupState.DEAD)
        val receipt = result.await()
        assertTrue(signalled.contains(android.system.OsConstants.SIGTERM))
        assertTrue(signalled.contains(android.system.OsConstants.SIGKILL))
        assertEquals("process_cancelled", receipt.string("code"))
        assertTrue(receipt.boolean("terminal_cancellation_acknowledged"))
        assertFalse(groupFile.exists())
    }

    @Test
    fun cancellationIsIsolatedByExactRemoteOperationId() {
        val processes = ConcurrentHashMap<String, FakeProcess>()
        val runner = BoundedProcessRunner { command, _ ->
            BoundedLaunchedProcess(
                processes.getValue(command.last()).also { it.launched.countDown() },
            )
        }
        val first = FakeProcess()
        val second = FakeProcess()
        processes["first"] = first
        processes["second"] = second
        val firstResult = AsyncReceipt {
            runner.run("remote-first", command("first"), null, 5_000L, 2000)
        }
        val secondResult = AsyncReceipt {
            runner.run("remote-second", command("second"), null, 5_000L, 2000)
        }
        assertTrue(first.launched.await(WAIT_MS, TimeUnit.MILLISECONDS))
        assertTrue(second.launched.await(WAIT_MS, TimeUnit.MILLISECONDS))

        runner.requestCancellation("remote-first")
        val cancelled = firstResult.await()

        assertTrue(first.destroyRequested.get())
        assertTrue(second.isAlive)
        assertFalse(second.destroyRequested.get())
        second.complete(0)
        assertEquals("process_cancelled", cancelled.string("code"))
        assertEquals("process_exited", secondResult.await().string("code"))
    }

    private class AsyncReceipt(block: () -> kotlinx.serialization.json.JsonObject) {
        val completed = CountDownLatch(1)
        private lateinit var value: kotlinx.serialization.json.JsonObject
        private val thread = Thread {
            value = block()
            completed.countDown()
        }.apply { start() }

        fun await(): kotlinx.serialization.json.JsonObject {
            assertTrue(completed.await(WAIT_MS, TimeUnit.MILLISECONDS))
            thread.join(WAIT_MS)
            return value
        }
    }

    private class FakeProcess(
        private val ignoreDestroy: Boolean = false,
    ) : Process() {
        val launched = CountDownLatch(1)
        val destroyRequested = AtomicBoolean(false)
        private val exited = CountDownLatch(1)
        private val alive = AtomicBoolean(true)
        private var code = 0

        override fun getOutputStream(): OutputStream = ByteArrayOutputStream()
        override fun getInputStream(): InputStream = ByteArrayInputStream(ByteArray(0))
        override fun getErrorStream(): InputStream = ByteArrayInputStream(ByteArray(0))

        override fun waitFor(): Int {
            exited.await()
            return code
        }

        override fun waitFor(timeout: Long, unit: TimeUnit): Boolean {
            return exited.await(timeout, unit)
        }

        override fun exitValue(): Int {
            if (alive.get()) throw IllegalThreadStateException("still running")
            return code
        }

        override fun destroy() = requestDestroy()

        override fun destroyForcibly(): Process {
            requestDestroy()
            return this
        }

        override fun isAlive(): Boolean = alive.get()

        fun complete(exitCode: Int) {
            code = exitCode
            alive.set(false)
            exited.countDown()
        }

        private fun requestDestroy() {
            destroyRequested.set(true)
            if (!ignoreDestroy) complete(143)
        }
    }

    private companion object {
        const val WAIT_MS = 3_000L

        fun command(label: String = "default") = listOf("program", label)

        fun kotlinx.serialization.json.JsonObject.string(name: String): String =
            getValue(name).jsonPrimitive.content

        fun kotlinx.serialization.json.JsonObject.boolean(name: String): Boolean =
            getValue(name).jsonPrimitive.boolean
    }
}

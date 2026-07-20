package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.charset.StandardCharsets
import java.util.LinkedHashSet
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

internal fun interface BoundedProcessLauncher {
    fun launch(command: List<String>, cwd: String?): BoundedLaunchedProcess
}

internal data class BoundedLaunchedProcess(
    val process: Process,
    val processGroupFile: File? = null,
)

internal enum class ProcessGroupState { ALIVE, DEAD, UNKNOWN }

internal interface ProcessGroupController {
    fun signal(groupId: Int, signal: Int, authorityUid: Int)

    fun state(groupId: Int, authorityUid: Int): ProcessGroupState
}

internal class BoundedProcessRunner(
    private val groupController: ProcessGroupController = AndroidProcessGroupController,
    private val launcher: BoundedProcessLauncher = BoundedProcessLauncher(::launchOwnedProcessGroup),
) {
    private val operations = ConcurrentHashMap<String, ProcessOperation>()
    private val pendingCancellationLock = Any()
    private val pendingCancellations = LinkedHashSet<String>()

    fun run(
        operationId: String,
        command: List<String>,
        cwd: String?,
        timeoutMs: Long,
        authorityUid: Int,
        onProcessStarted: () -> Unit = {},
    ): JsonObject {
        val startedAt = System.nanoTime()
        validate(operationId, command, cwd, timeoutMs)?.let { message ->
            return failure("invalid_request", message, operationId, authorityUid, startedAt)
        }
        val operation = ProcessOperation(operationId, authorityUid, groupController)
        if (operations.putIfAbsent(operationId, operation) != null) {
            return failure(
                "duplicate_operation",
                "The exact command operation is already active.",
                operationId,
                authorityUid,
                startedAt,
            )
        }
        if (consumePendingCancellation(operationId)) operation.requestCancellation()

        return try {
            if (operation.cancellationRequested) {
                cancelledReceipt(operationId, false, authorityUid, startedAt)
            } else {
                execute(
                    operation = operation,
                    command = command,
                    cwd = cwd,
                    timeoutMs = timeoutMs,
                    authorityUid = authorityUid,
                    startedAt = startedAt,
                    onProcessStarted = onProcessStarted,
                )
            }
        } finally {
            operation.close()
            operations.remove(operationId, operation)
        }
    }

    /** Requests cancellation of exactly one operation without blocking the lifecycle thread. */
    fun requestCancellation(operationId: String): JsonObject {
        val operation = operations[operationId]
        if (operation == null) {
            rememberPendingCancellation(operationId)
            return buildJsonObject {
                put("ok", true)
                put("code", "cancellation_registered")
                put("operation_id", operationId)
                put("terminal_cancellation_acknowledged", false)
            }
        }
        operation.requestCancellation()
        return buildJsonObject {
            put("ok", true)
            put("code", "cancellation_requested")
            put("operation_id", operationId)
            put("process_started", operation.processStarted)
            put("terminal_cancellation_acknowledged", false)
        }
    }

    private fun execute(
        operation: ProcessOperation,
        command: List<String>,
        cwd: String?,
        timeoutMs: Long,
        authorityUid: Int,
        startedAt: Long,
        onProcessStarted: () -> Unit,
    ): JsonObject {
        val launched = try {
            launcher.launch(command, cwd)
        } catch (error: Throwable) {
            return failure(
                "launch_failed",
                error.message ?: error.javaClass.simpleName,
                operation.id,
                authorityUid,
                startedAt,
            )
        }
        val process = launched.process
        operation.attach(launched)
        if (operation.processStarted) onProcessStarted()

        val output = BoundedOutput(MAX_OUTPUT_BYTES)
        val readerFailure = AtomicReference<Throwable?>(null)
        val reader = Thread({ readOutput(process, output, readerFailure) }, COMMAND_OUTPUT_THREAD)
        reader.start()

        val completedNaturally = try {
            waitForExitOrCancellation(process, operation, timeoutMs)
        } catch (error: InterruptedException) {
            Thread.currentThread().interrupt()
            operation.requestCancellation()
            false
        }
        if (!completedNaturally || operation.cancellationRequested) {
            operation.markTimedOutUnlessCancelled()
            operation.terminateAndAwait()
        }
        reader.join(READER_JOIN_MS)
        if (reader.isAlive) reader.interrupt()
        val cancelled = operation.cancellationRequested
        val timedOut = operation.timedOut && !cancelled
        val terminationConfirmed = operation.isTerminal()
        val readerError = readerFailure.get()
        return buildJsonObject {
            put("ok", completedNaturally && !cancelled && readerError == null)
            put(
                "code",
                when {
                    cancelled -> "process_cancelled"
                    timedOut -> "process_timed_out"
                    else -> "process_exited"
                },
            )
            val exitCode = if (terminationConfirmed) runCatching(process::exitValue).getOrNull() else null
            if (exitCode != null) put("exit_code", exitCode) else put("exit_code", JsonNull)
            put("timed_out", timedOut)
            put("cancelled", cancelled)
            put("process_started", true)
            put("terminal_cancellation_acknowledged", cancelled && terminationConfirmed)
            put("operation_id", operation.id)
            put("output", output.text())
            put("output_truncated", output.truncated)
            put("output_bytes", output.totalBytes)
            put("authority_uid", authorityUid)
            put("duration_ms", elapsedMs(startedAt))
            readerError?.let { put("reader_error", it.message ?: it.javaClass.simpleName) }
        }
    }

    private fun validate(
        operationId: String,
        command: List<String>,
        cwd: String?,
        timeoutMs: Long,
    ): String? = when {
        operationId.isBlank() || operationId.length > MAX_OPERATION_ID_CHARS ->
            "Operation id is blank or too long."
        command.isEmpty() || command.first().isBlank() -> "Program must not be blank."
        command.size > MAX_ARGS + 1 -> "Too many arguments."
        command.any { '\u0000' in it } -> "Program or argument contains a NUL byte."
        timeoutMs !in MIN_TIMEOUT_MS..MAX_TIMEOUT_MS -> "Timeout is outside the supported range."
        cwd != null && (!File(cwd).isAbsolute || !File(cwd).isDirectory) ->
            "Working directory must be an existing absolute directory."
        else -> null
    }

    private fun rememberPendingCancellation(operationId: String) {
        synchronized(pendingCancellationLock) {
            pendingCancellations += operationId
            while (pendingCancellations.size > MAX_PENDING_CANCELLATIONS) {
                pendingCancellations.remove(pendingCancellations.first())
            }
        }
    }

    private fun consumePendingCancellation(operationId: String): Boolean =
        synchronized(pendingCancellationLock) { pendingCancellations.remove(operationId) }

    @Throws(InterruptedException::class)
    private fun waitForExitOrCancellation(
        process: Process,
        operation: ProcessOperation,
        timeoutMs: Long,
    ): Boolean {
        val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMs)
        while (!operation.isTerminal() && !operation.cancellationRequested) {
            val remainingNanos = deadline - System.nanoTime()
            if (remainingNanos <= 0L) return false
            val waitMs = minOf(
                PROCESS_POLL_MS,
                TimeUnit.NANOSECONDS.toMillis(remainingNanos).coerceAtLeast(1L),
            )
            if (process.isAlive) {
                process.waitFor(waitMs, TimeUnit.MILLISECONDS)
            } else {
                Thread.sleep(waitMs)
            }
        }
        return operation.isTerminal()
    }

    private fun readOutput(
        process: Process,
        output: BoundedOutput,
        readerFailure: AtomicReference<Throwable?>,
    ) {
        try {
            process.inputStream.use { input ->
                val buffer = ByteArray(8192)
                while (true) {
                    val read = input.read(buffer)
                    if (read < 0) break
                    output.write(buffer, 0, read)
                }
            }
        } catch (error: Throwable) {
            readerFailure.set(error)
        }
    }

    private fun failure(
        code: String,
        message: String,
        operationId: String,
        authorityUid: Int,
        startedAt: Long,
    ): JsonObject = buildJsonObject {
        put("ok", false)
        put("code", code)
        put("message", message)
        put("operation_id", operationId)
        put("process_started", false)
        put("authority_uid", authorityUid)
        put("duration_ms", elapsedMs(startedAt))
    }

    private fun cancelledReceipt(
        operationId: String,
        processStarted: Boolean,
        authorityUid: Int,
        startedAt: Long,
    ): JsonObject = buildJsonObject {
        put("ok", false)
        put("code", "process_cancelled")
        put("operation_id", operationId)
        put("process_started", processStarted)
        put("cancelled", true)
        put("terminal_cancellation_acknowledged", true)
        put("authority_uid", authorityUid)
        put("duration_ms", elapsedMs(startedAt))
    }

    private class ProcessOperation(
        val id: String,
        private val authorityUid: Int,
        private val groupController: ProcessGroupController,
    ) {
        private val lock = Any()
        private var process: Process? = null
        private var processGroupFile: File? = null
        private var processGroupId: Int? = null
        private val cancelled = AtomicBoolean(false)
        private val timeout = AtomicBoolean(false)

        val cancellationRequested: Boolean get() = cancelled.get()
        val timedOut: Boolean get() = timeout.get()
        val processStarted: Boolean get() = synchronized(lock) { process != null }

        fun attach(started: BoundedLaunchedProcess) {
            val cancelNow = synchronized(lock) {
                check(process == null)
                process = started.process
                processGroupFile = started.processGroupFile
                captureProcessGroupLocked()
                cancelled.get()
            }
            if (cancelNow) destroyOwnedProcessTree()
        }

        fun requestCancellation() {
            cancelled.set(true)
            destroyOwnedProcessTree()
        }

        fun markTimedOutUnlessCancelled() {
            if (!cancelled.get()) timeout.set(true)
        }

        fun terminateAndAwait() {
            while (true) {
                destroyOwnedProcessTree()
                val root = synchronized(lock) { process }
                if (isTerminal()) return
                root?.let { running ->
                    runCatching { running.waitFor(PROCESS_POLL_MS, TimeUnit.MILLISECONDS) }
                }
            }
        }

        private fun destroyOwnedProcessTree() {
            val (root, groupId) = synchronized(lock) {
                captureProcessGroupLocked()
                process to processGroupId
            }
            groupId?.let { ownedGroup ->
                runCatching { groupController.signal(ownedGroup, OsConstants.SIGTERM, authorityUid) }
            }
            root?.let { running ->
                runCatching { running.destroy() }
                runCatching { if (running.isAlive) running.destroyForcibly() }
            }
            groupId?.let { ownedGroup ->
                runCatching { groupController.signal(ownedGroup, OsConstants.SIGKILL, authorityUid) }
            }
        }

        fun isTerminal(): Boolean {
            val (rootAlive, groupId) = synchronized(lock) {
                captureProcessGroupLocked()
                (process?.isAlive == true) to processGroupId
            }
            if (rootAlive) return false
            return groupId == null ||
                groupController.state(groupId, authorityUid) == ProcessGroupState.DEAD
        }

        fun close() {
            synchronized(lock) { processGroupFile }.let { file ->
                if (file != null) runCatching { file.delete() }
            }
        }

        private fun captureProcessGroupLocked() {
            if (processGroupId != null) return
            val file = processGroupFile ?: return
            val raw = runCatching { file.readText().trim() }.getOrNull() ?: return
            processGroupId = raw.toIntOrNull()?.takeIf { it > 1 }
        }
    }

    private class BoundedOutput(private val limit: Int) {
        private val bytes = ByteArrayOutputStream(limit)
        var totalBytes: Long = 0
            private set
        var truncated: Boolean = false
            private set

        @Synchronized
        fun write(buffer: ByteArray, offset: Int, length: Int) {
            totalBytes += length
            val remaining = limit - bytes.size()
            if (remaining > 0) bytes.write(buffer, offset, minOf(length, remaining))
            if (length > remaining) truncated = true
        }

        @Synchronized
        fun text(): String = bytes.toString(StandardCharsets.UTF_8.name())
    }

    private companion object {
        const val MAX_ARGS = 16
        const val MAX_OPERATION_ID_CHARS = 8_192
        const val MAX_OUTPUT_BYTES = 64 * 1024
        const val MAX_PENDING_CANCELLATIONS = 256
        const val MIN_TIMEOUT_MS = 100L
        const val MAX_TIMEOUT_MS = 60_000L
        const val READER_JOIN_MS = 2_000L
        const val PROCESS_POLL_MS = 100L
        const val COMMAND_OUTPUT_THREAD = "SGT-PhoneControl-command-output"
    }
}

private fun launchOwnedProcessGroup(command: List<String>, cwd: String?): BoundedLaunchedProcess {
    val groupFile = File.createTempFile("sgt-phone-control-", ".pgrp")
    return try {
        val script = "printf '%s\\n' \"\$\$\" > \"\$1\" || exit 125; shift; exec \"\$@\""
        val wrapped = listOf(
            "/system/bin/setsid",
            "/system/bin/sh",
            "-c",
            script,
            "sgt-phone-control",
            groupFile.absolutePath,
        ) + command
        val process = ProcessBuilder(wrapped)
            .directory(cwd?.let(::File))
            .redirectErrorStream(true)
            .start()
        BoundedLaunchedProcess(process, groupFile)
    } catch (error: Throwable) {
        runCatching { groupFile.delete() }
        throw error
    }
}

private object AndroidProcessGroupController : ProcessGroupController {
    override fun signal(groupId: Int, signal: Int, authorityUid: Int) {
        if (authorityUid == android.os.Process.ROOT_UID) {
            runRootKill(groupId, signal)
        } else {
            Os.kill(-groupId, signal)
        }
    }

    override fun state(groupId: Int, authorityUid: Int): ProcessGroupState {
        if (authorityUid == android.os.Process.ROOT_UID) {
            return when (runRootKill(groupId, 0)) {
                0 -> ProcessGroupState.ALIVE
                1 -> ProcessGroupState.DEAD
                else -> ProcessGroupState.UNKNOWN
            }
        }
        return try {
            Os.kill(-groupId, 0)
            ProcessGroupState.ALIVE
        } catch (error: ErrnoException) {
            if (error.errno == OsConstants.ESRCH) {
                ProcessGroupState.DEAD
            } else {
                ProcessGroupState.UNKNOWN
            }
        }
    }

    private fun runRootKill(groupId: Int, signal: Int): Int? {
        val command = "exec /system/bin/toybox kill -s $signal -$groupId"
        val process = try {
            ProcessBuilder("su", "-c", command).redirectErrorStream(true).start()
        } catch (_: Throwable) {
            return null
        }
        return try {
            if (!process.waitFor(ROOT_SIGNAL_TIMEOUT_MS, TimeUnit.MILLISECONDS)) {
                process.destroyForcibly()
                null
            } else {
                process.exitValue()
            }
        } catch (_: Throwable) {
            runCatching { process.destroyForcibly() }
            null
        }
    }

    private const val ROOT_SIGNAL_TIMEOUT_MS = 2_000L
}

internal val defaultBoundedProcessRunner = BoundedProcessRunner()

private fun elapsedMs(startedAt: Long): Long =
    TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - startedAt)

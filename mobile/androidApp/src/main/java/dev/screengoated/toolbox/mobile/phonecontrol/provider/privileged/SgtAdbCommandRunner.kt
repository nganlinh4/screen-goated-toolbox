package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import io.github.muntashirakon.adb.AdbStream
import java.io.ByteArrayOutputStream
import java.nio.charset.StandardCharsets
import java.util.LinkedHashSet
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class SgtAdbCommandRunner(
    private val manager: SgtAdbConnectionManager,
) {
    private val operations = ConcurrentHashMap<String, Operation>()
    private val pendingCancellationLock = Any()
    private val pendingCancellations = LinkedHashSet<String>()

    fun run(
        operationId: String,
        program: String,
        args: List<String>,
        cwd: String?,
        timeoutMs: Long,
    ): JsonObject {
        val startedAt = System.nanoTime()
        validateSgtAdbCommandRequest(operationId, program, args, cwd, timeoutMs)?.let { message ->
            return failure("invalid_request", message, operationId, startedAt)
        }
        val operation = Operation(operationId)
        if (operations.putIfAbsent(operationId, operation) != null) {
            return failure(
                "duplicate_operation",
                "The exact ADB operation is already active.",
                operationId,
                startedAt,
            )
        }
        if (consumePendingCancellation(operationId)) operation.cancel()
        return try {
            if (operation.cancelled.get()) {
                receipt(operation, null, BoundedAdbOutput(), null, startedAt)
            } else {
                execute(operation, program, args, cwd, timeoutMs, startedAt)
            }
        } finally {
            operation.closeStream()
            operations.remove(operationId, operation)
        }
    }

    fun cancel(operationId: String): JsonObject {
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
        operation.cancel()
        return buildJsonObject {
            put("ok", true)
            put("code", "cancellation_requested")
            put("operation_id", operationId)
            put("process_started", operation.started.get())
            put("terminal_cancellation_acknowledged", false)
        }
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
        synchronized(pendingCancellationLock) {
            pendingCancellations.remove(operationId)
        }

    private fun execute(
        operation: Operation,
        program: String,
        args: List<String>,
        cwd: String?,
        timeoutMs: Long,
        startedAt: Long,
    ): JsonObject {
        val marker = "__SGT_ADB_RC_${UUID.randomUUID().toString().replace("-", "")}__"
        val destination = "shell:${commandScript(program, args, cwd, marker)}"
        val stream = try {
            manager.openStream(destination)
        } catch (error: Throwable) {
            return failure(
                "launch_failed",
                error.message ?: error.javaClass.simpleName,
                operation.id,
                startedAt,
            )
        }
        operation.attach(stream)
        val output = BoundedAdbOutput()
        val readerFailure = AtomicReference<Throwable?>(null)
        val reader = Thread(
            {
                runCatching { stream.openInputStream().use { input -> output.read(input) } }
                    .onFailure { error ->
                        if (!operation.cancelled.get() && !operation.timedOut.get()) {
                            readerFailure.set(error)
                        }
                    }
            },
            "sgt-adb-output",
        )
        reader.start()
        val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMs)
        while (reader.isAlive && !operation.cancelled.get()) {
            if (System.nanoTime() >= deadline) {
                operation.timedOut.set(true)
                operation.closeStream()
                break
            }
            reader.join(POLL_MS)
        }
        if (operation.cancelled.get()) operation.closeStream()
        reader.join(READER_SETTLE_MS)
        if (reader.isAlive) {
            operation.closeStream()
            reader.interrupt()
        }
        val exitCode = output.exitCode(marker)
        operation.closeStream()
        return receipt(operation, exitCode, output, readerFailure.get(), startedAt)
    }

    private fun receipt(
        operation: Operation,
        exitCode: Int?,
        output: BoundedAdbOutput,
        readerError: Throwable?,
        startedAt: Long,
    ): JsonObject = buildJsonObject {
        val cancelled = operation.cancelled.get()
        val timedOut = operation.timedOut.get() && !cancelled
        put("ok", !cancelled && !timedOut && exitCode != null && readerError == null)
        put(
            "code",
            when {
                cancelled -> "process_cancelled"
                timedOut -> "process_timed_out"
                exitCode == null -> "stream_closed_without_status"
                else -> "process_exited"
            },
        )
        if (exitCode == null) put("exit_code", JsonNull) else put("exit_code", exitCode)
        put("timed_out", timedOut)
        put("cancelled", cancelled)
        put("process_started", operation.started.get())
        put(
            "terminal_cancellation_acknowledged",
            cancelled && operation.stream.get()?.isClosed != false,
        )
        put("operation_id", operation.id)
        put("output", output.visibleText())
        put("output_truncated", output.truncated)
        put("output_bytes", output.totalBytes)
        put("authority_uid", ADB_SHELL_UID)
        put("duration_ms", elapsedMs(startedAt))
        readerError?.let { put("reader_error", it.message ?: it.javaClass.simpleName) }
    }

    private fun failure(
        code: String,
        message: String,
        operationId: String,
        startedAt: Long,
    ): JsonObject = buildJsonObject {
        put("ok", false)
        put("code", code)
        put("message", message)
        put("exit_code", JsonNull)
        put("timed_out", false)
        put("cancelled", false)
        put("process_started", false)
        put("terminal_cancellation_acknowledged", false)
        put("operation_id", operationId)
        put("output", "")
        put("output_truncated", false)
        put("output_bytes", 0)
        put("authority_uid", ADB_SHELL_UID)
        put("duration_ms", elapsedMs(startedAt))
    }

    private class Operation(val id: String) {
        val cancelled = AtomicBoolean(false)
        val timedOut = AtomicBoolean(false)
        val started = AtomicBoolean(false)
        val stream = AtomicReference<AdbStream?>()

        fun attach(value: AdbStream) {
            stream.set(value)
            started.set(true)
            if (cancelled.get()) closeStream()
        }

        fun cancel() {
            cancelled.set(true)
            closeStream()
        }

        fun closeStream() {
            stream.get()?.let { runCatching(it::close) }
        }
    }
}

internal fun validateSgtAdbCommandRequest(
    operationId: String,
    program: String,
    args: List<String>,
    cwd: String?,
    timeoutMs: Long,
): String? = when {
    operationId.isBlank() || operationId.length > MAX_OPERATION_ID_CHARS ->
        "Operation id is blank or too long."
    program.isBlank() || program.utf8Size() > MAX_PROGRAM_BYTES || '\u0000' in program ->
        "Program is invalid."
    args.size > MAX_ARGS ||
        args.any { it.utf8Size() > MAX_ARG_BYTES || '\u0000' in it } ||
        args.sumOf { it.utf8Size() } > MAX_TOTAL_ARG_BYTES ->
        "Arguments are invalid."
    cwd != null &&
        (!cwd.startsWith('/') || cwd.utf8Size() > MAX_CWD_BYTES || '\u0000' in cwd) ->
        "Working directory is invalid."
    timeoutMs !in MIN_TIMEOUT_MS..MAX_TIMEOUT_MS -> "Timeout is outside the supported range."
    else -> null
}

internal fun commandScript(
    program: String,
    args: List<String>,
    cwd: String?,
    statusMarker: String,
): String {
    val invocation = (listOf(program) + args).joinToString(" ", transform = ::shellQuote)
    val workingDirectory = cwd?.let { "cd ${shellQuote(it)} || exit 125; " }.orEmpty()
    return "$workingDirectory$invocation; sgt_status=\$?; " +
        "printf '\\n%s%d\\n' ${shellQuote(statusMarker)} \"\$sgt_status\"; exit \"\$sgt_status\""
}

internal fun shellQuote(value: String): String = "'${value.replace("'", "'\\''")}'"

private class BoundedAdbOutput {
    private val prefix = ByteArrayOutputStream()
    private val tail = ArrayDeque<Byte>()
    var totalBytes: Long = 0
        private set

    val truncated: Boolean
        get() = totalBytes > MAX_OUTPUT_BYTES

    fun read(input: java.io.InputStream) {
        val buffer = ByteArray(READ_BUFFER_BYTES)
        while (true) {
            val count = input.read(buffer)
            if (count < 0) return
            append(buffer, count)
        }
    }

    fun exitCode(marker: String): Int? {
        val text = tail.toByteArray().toString(StandardCharsets.UTF_8)
        return Regex("${Regex.escape(marker)}(-?\\d+)\\s*$")
            .find(text)
            ?.groupValues
            ?.get(1)
            ?.toIntOrNull()
    }

    fun visibleText(): String {
        val text = prefix.toByteArray().toString(StandardCharsets.UTF_8)
        return text.replace(STATUS_SUFFIX, "").trimEnd('\r', '\n')
    }

    private fun append(bytes: ByteArray, count: Int) {
        totalBytes += count
        val remaining = (MAX_OUTPUT_BYTES - prefix.size()).coerceAtLeast(0)
        if (remaining > 0) prefix.write(bytes, 0, minOf(count, remaining))
        repeat(count) { index ->
            tail.addLast(bytes[index])
            if (tail.size > TAIL_BYTES) tail.removeFirst()
        }
    }
}

private fun ArrayDeque<Byte>.toByteArray(): ByteArray =
    ByteArray(size).also { output -> forEachIndexed { index, byte -> output[index] = byte } }

private fun elapsedMs(startedAt: Long): Long =
    TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - startedAt)

private fun String.utf8Size(): Int = toByteArray(StandardCharsets.UTF_8).size

private val STATUS_SUFFIX = Regex("\\n__SGT_ADB_RC_[0-9a-f]+__-?\\d+\\s*$")
private const val ADB_SHELL_UID = 2000
private const val MAX_OUTPUT_BYTES = 64 * 1_024
private const val TAIL_BYTES = 512
private const val READ_BUFFER_BYTES = 8_192
private const val POLL_MS = 25L
private const val READER_SETTLE_MS = 1_000L
private const val MIN_TIMEOUT_MS = 100L
private const val MAX_TIMEOUT_MS = 60_000L
private const val MAX_OPERATION_ID_CHARS = 8_192
private const val MAX_PENDING_CANCELLATIONS = 256
private const val MAX_ARGS = 16
private const val MAX_PROGRAM_BYTES = 1_024
private const val MAX_ARG_BYTES = 4_096
private const val MAX_TOTAL_ARG_BYTES = 16_384
private const val MAX_CWD_BYTES = 4_096

package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Context
import android.os.Process
import android.os.SystemClock
import android.util.Log
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit

/** Diagnostics are evidence only and can never alter Phone Control state. */
internal object PhoneControlLog {
    @Volatile
    private var diagnosticDirectory: File? = null

    private val writer = ThreadPoolExecutor(
        1,
        1,
        0L,
        TimeUnit.MILLISECONDS,
        LinkedBlockingQueue(MAX_PENDING_RECORDS),
        { task ->
            Thread(task, "phone-control-diagnostics").apply {
                isDaemon = true
                priority = Thread.MIN_PRIORITY
            }
        },
        ThreadPoolExecutor.DiscardOldestPolicy(),
    )

    fun initialize(context: Context) {
        if (diagnosticDirectory != null) return
        synchronized(this) {
            if (diagnosticDirectory == null) {
                diagnosticDirectory = (
                    context.getExternalFilesDir(DIRECTORY_NAME)
                        ?: File(context.filesDir, DIRECTORY_NAME)
                    ).also { directory -> runCatching { directory.mkdirs() } }
            }
        }
        record("I", INTERNAL_TAG, "diagnostics_initialized")
    }

    fun d(tag: String, message: String): Int = write("D", tag, message) {
        Log.d(tag, message)
    }

    fun i(tag: String, message: String): Int = write("I", tag, message) {
        Log.i(tag, message)
    }

    fun w(tag: String, message: String): Int = write("W", tag, message) {
        Log.w(tag, message)
    }

    fun e(tag: String, message: String): Int = write("E", tag, message) {
        Log.e(tag, message)
    }

    fun e(tag: String, message: String, error: Throwable?): Int = write(
        level = "E",
        tag = tag,
        message = message,
        throwableType = error?.javaClass?.name,
    ) {
        Log.e(tag, message, error)
    }

    private inline fun write(
        level: String,
        tag: String,
        message: String,
        throwableType: String? = null,
        logcatWrite: () -> Int,
    ): Int {
        record(level, tag, message, throwableType)
        return runCatching(logcatWrite).getOrDefault(0)
    }

    private fun record(
        level: String,
        tag: String,
        message: String,
        throwableType: String? = null,
    ) {
        val directory = diagnosticDirectory ?: return
        val timestamp = System.currentTimeMillis()
        val elapsed = SystemClock.elapsedRealtime()
        val sourceThread = Thread.currentThread().name.take(MAX_THREAD_CHARS)
        val safeTag = normalizeDiagnosticField(tag, MAX_TAG_CHARS)
        val safeMessage = normalizeDiagnosticField(message, MAX_MESSAGE_CHARS)
        val safeThrowableType = throwableType?.let {
            normalizeDiagnosticField(it, MAX_THROWABLE_CHARS)
        }
        runCatching {
            writer.execute {
                runCatching {
                    val record = JSONObject()
                        .put("timestamp_ms", timestamp)
                        .put("elapsed_ms", elapsed)
                        .put("pid", Process.myPid())
                        .put("thread", sourceThread)
                        .put("level", level)
                        .put("tag", safeTag)
                        .put("event", safeMessage)
                    if (safeThrowableType != null) {
                        record.put("throwable_type", safeThrowableType)
                    }
                    appendRecord(directory, record.toString())
                }
            }
        }
    }

    private fun appendRecord(directory: File, json: String) {
        if (!directory.exists() && !directory.mkdirs()) return
        val current = File(directory, CURRENT_FILE_NAME)
        if (current.length() >= MAX_FILE_BYTES) {
            val previous = File(directory, PREVIOUS_FILE_NAME)
            previous.delete()
            if (!current.renameTo(previous)) current.delete()
        }
        FileOutputStream(current, true).bufferedWriter(Charsets.UTF_8).use { output ->
            output.append(json)
            output.newLine()
        }
    }

    internal fun normalizeDiagnosticField(value: String, maxChars: Int): String = value
        .asSequence()
        .map { character -> if (character.isISOControl()) ' ' else character }
        .joinToString(separator = "")
        .replace(WHITESPACE_RUN, " ")
        .trim()
        .take(maxChars)

    private const val INTERNAL_TAG = "SGTPhoneControlDiagnostics"
    private const val DIRECTORY_NAME = "phone-control-diagnostics"
    private const val CURRENT_FILE_NAME = "events.jsonl"
    private const val PREVIOUS_FILE_NAME = "events.previous.jsonl"
    private const val MAX_PENDING_RECORDS = 512
    private const val MAX_FILE_BYTES = 1_048_576L
    private const val MAX_TAG_CHARS = 96
    private const val MAX_MESSAGE_CHARS = 1_024
    private const val MAX_THROWABLE_CHARS = 192
    private const val MAX_THREAD_CHARS = 96
    private val WHITESPACE_RUN = Regex(" +")
}

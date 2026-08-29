package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.util.Log
import java.io.File
import java.io.RandomAccessFile
import org.json.JSONObject

internal class CreationDiagnostics(context: Context) {
    private val directory = File(context.filesDir, "creation/diagnostics")
    private val current = File(directory, "events.jsonl")
    private val previous = File(directory, "events.previous.jsonl")
    private val lockFile = File(directory, "events.lock")

    fun event(
        name: String,
        tool: String? = null,
        jobId: String? = null,
        stage: String? = null,
        failureCategory: String? = null,
    ) {
        val category = failureCategory?.let(::publicCreationFailureCategory)
        val summary = buildString {
            append(name)
            tool?.let { append(" tool=").append(it) }
            stage?.let { append(" stage=").append(it) }
            category?.let { append(" failure=").append(it) }
        }
        if (category == null) Log.i(TAG, summary) else Log.w(TAG, summary)
        runCatching {
            append(record(name, tool, jobId, stage, category))
        }
            .onFailure { Log.w(TAG, "journal_write_failed") }
    }

    private fun record(
        name: String,
        tool: String?,
        jobId: String?,
        stage: String?,
        failureCategory: String?,
    ): String = JSONObject().apply {
        put("timeMs", System.currentTimeMillis())
        put("event", fixedToken(name))
        tool?.let { put("tool", fixedToken(it)) }
        jobId?.let { put("job", it.takeLast(16).replace(NON_TOKEN, "_")) }
        stage?.let { put("stage", fixedToken(it)) }
        failureCategory?.let { put("failure", it) }
    }.toString() + "\n"

    private fun append(line: String) {
        directory.mkdirs()
        RandomAccessFile(lockFile, "rw").use { lock ->
            lock.channel.lock().use {
                if (current.length() + line.length > MAXIMUM_BYTES) {
                    previous.delete()
                    if (!current.renameTo(previous)) current.writeText("")
                }
                current.appendText(line)
            }
        }
    }

    companion object {
        private const val TAG = "CreationRuntime"
        private const val MAXIMUM_BYTES = 256 * 1024L
        private val NON_TOKEN = Regex("[^A-Za-z0-9_.-]")

        private fun fixedToken(value: String): String = value.take(40).replace(NON_TOKEN, "_")
    }
}

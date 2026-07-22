package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.os.Process
import android.util.Log
import java.io.File
import java.io.RandomAccessFile
import org.json.JSONObject

internal class CreationDiagnostics(context: Context, private val scope: String) {
    private val directory = File(context.filesDir, "creation/diagnostics")
    private val current = File(directory, "events.jsonl")
    private val previous = File(directory, "events.previous.jsonl")
    private val lockFile = File(directory, "events.lock")

    fun event(
        name: String,
        tool: String? = null,
        slot: Int? = null,
        jobId: String? = null,
        stage: String? = null,
        failure: Throwable? = null,
        failureMessage: String? = failure?.message,
    ) {
        val category = failureMessage?.let(::failureCategory)
        val summary = buildString {
            append(scope).append(' ').append(name)
            tool?.let { append(" tool=").append(it) }
            slot?.let { append(" slot=").append(it) }
            stage?.let { append(" stage=").append(it) }
            category?.let { append(" failure=").append(it) }
        }
        if (category == null) Log.i(TAG, summary) else Log.w(TAG, summary)
        runCatching { append(record(name, tool, slot, jobId, stage, category)) }
            .onFailure { Log.w(TAG, "$scope journal_write_failed") }
    }

    private fun record(
        name: String,
        tool: String?,
        slot: Int?,
        jobId: String?,
        stage: String?,
        failureCategory: String?,
    ): String = JSONObject().apply {
        put("timeMs", System.currentTimeMillis())
        put("pid", Process.myPid())
        put("scope", fixedToken(scope))
        put("event", fixedToken(name))
        tool?.let { put("tool", fixedToken(it)) }
        slot?.let { put("slot", it) }
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

        fun failureCategory(message: String): String {
            val normalized = message.lowercase()
            return when {
                "rate limit" in normalized || "too many" in normalized -> "rate_limit"
                "timed out" in normalized || "timeout" in normalized -> "timeout"
                "cooling down" in normalized -> "cooldown"
                "mailbox" in normalized || "email" in normalized -> "mailbox"
                "credit" in normalized -> "credits"
                "upload" in normalized || "image" in normalized -> "upload"
                "control" in normalized || "selector" in normalized ||
                    "onboarding" in normalized || "smart mesh" in normalized ||
                    "topology mode" in normalized -> "page_control"
                "workspace became ready" in normalized || "workspace unavailable" in normalized ->
                    "workspace_unavailable"
                "output" in normalized || "result" in normalized -> "output"
                "worker" in normalized || "binder" in normalized -> "worker"
                else -> "unexpected"
            }
        }

        private fun fixedToken(value: String): String = value.take(40).replace(NON_TOKEN, "_")
    }
}

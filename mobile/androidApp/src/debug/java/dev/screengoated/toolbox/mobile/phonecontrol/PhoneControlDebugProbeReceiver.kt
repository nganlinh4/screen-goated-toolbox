package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.SystemClock
import android.util.Base64
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlDispatcherToolExecutor
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolAdmission
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolCompletionEvent
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolController
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolRequest
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolDispatcher
import java.io.File
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

/** Debug-only host bridge for exercising the real in-process tool stack over adb. */
class PhoneControlDebugProbeReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val requestId = intent.getStringExtra(EXTRA_REQUEST_ID).orEmpty()
        if (!REQUEST_ID.matches(requestId)) return
        if (intent.action == CANCEL_ACTION) {
            PROBE_ADMISSION.cancel(requestId)
            deleteProbeReceipt(context.applicationContext, requestId)
            return
        }
        if (intent.action != ACTION) return
        val pendingResult = goAsync()
        val appContext = context.applicationContext
        pruneExpiredProbeReceipts(appContext)
        val operation = DebugProbeOperation(requestId)
        lateinit var lease: DebugProbeLease
        val job = PROBE_SCOPE.launch(start = CoroutineStart.LAZY) {
            try {
                execute(appContext, intent, operation)
            } finally {
                PROBE_ADMISSION.release(lease)
                pendingResult.finish()
            }
        }
        val admitted = PROBE_ADMISSION.tryAdmit(requestId, operation)
        if (admitted != null) {
            lease = admitted
            job.start()
        } else {
            job.cancel()
            val activeRequestId = PROBE_ADMISSION.activeRequestId()
            if (activeRequestId != requestId) {
                runCatching {
                    writeBusyReceipt(appContext, requestId)
                }
            }
            pendingResult.finish()
        }
    }

    private suspend fun execute(
        context: Context,
        intent: Intent,
        operation: DebugProbeOperation,
    ) {
        var mutating = false
        var dispatchAdmitted = false
        val response = try {
            val tool = requireNotNull(intent.getStringExtra(EXTRA_TOOL))
                .takeIf(String::isNotBlank) ?: error("tool is required")
            mutating = debugProbeMutationRequired(tool)
            check(debugProbeAllows(tool, intent.getBooleanExtra(EXTRA_ALLOW_MUTATION, false))) {
                "mutating debug probe requires explicit mutation acknowledgement"
            }
            val arguments = decodeArguments(intent.getStringExtra(EXTRA_ARGUMENTS_BASE64))
            val completions = Channel<PhoneControlToolCompletionEvent>(capacity = 1)
            val executor = PhoneControlDispatcherToolExecutor(
                boundary = PROBE_DISPATCHER.getOrCreate {
                    PhoneControlToolDispatcher(context.applicationContext)
                },
                scope = PROBE_SCOPE,
            )
            val controller = PhoneControlToolController(executor, PROBE_SCOPE, completions)
            val request = PhoneControlToolRequest(
                id = operation.requestId,
                name = tool,
                arguments = arguments,
                turnId = 1,
                generation = 1,
            )
            check(controller.dispatch(request) == PhoneControlToolAdmission.ACCEPTED) {
                "debug probe production controller rejected an unowned request"
            }
            dispatchAdmitted = true
            operation.attachCancellation {
                controller.cancel(listOf(operation.requestId))
            }
            var timedOut = false
            var event = withTimeoutOrNull(PROBE_EXECUTION_TIMEOUT_MS) { completions.receive() }
            if (event == null) {
                timedOut = true
                operation.requestCancellation(suppressFutureReceipt = false)
                event = completions.receive()
            }
            val terminalEvent = requireNotNull(event)
            val completed = requireNotNull(controller.takeCompletion(terminalEvent)) {
                "debug probe received a stale production completion"
            }
            if (timedOut) {
                probeFailure(
                    errorName = "ProbeTimeout",
                    message = "debug probe exceeded its bounded execution window",
                    effectMayHaveOccurred = completed.result.certainty !=
                        PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
                )
            } else {
                buildJsonObject {
                    put("response", completed.result.response)
                    put("mutating", mutating)
                    put("refresh_screen_frame", completed.result.refreshScreenFrame)
                }
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Throwable) {
            probeFailure(
                errorName = error.javaClass.simpleName,
                message = error.message.orEmpty().take(MAX_ERROR_CHARS),
                effectMayHaveOccurred = dispatchAdmitted && mutating,
            )
        }
        operation.publishReceiptIfAllowed {
            writeAtomic(context, operation.requestId, response.toString())
        }
    }

    private fun decodeArguments(encoded: String?): JsonObject {
        if (encoded.isNullOrBlank()) return JsonObject(emptyMap())
        val bytes = Base64.decode(encoded, Base64.NO_WRAP)
        require(bytes.size <= MAX_ARGUMENT_BYTES) { "arguments exceed debug probe limit" }
        return JSON.parseToJsonElement(bytes.decodeToString()).jsonObject
    }

    private fun writeAtomic(context: Context, requestId: String, value: String) {
        synchronized(RECEIPT_LOCK) {
            val directory = probeOutputDirectory(context)
            val target = File(directory, "$requestId.json")
            val temporary = File(directory, ".$requestId.tmp")
            scheduleProbeReceiptExpiry(context, requestId)
            temporary.writeText(value, Charsets.UTF_8)
            check(temporary.renameTo(target)) { "debug probe result rename failed" }
        }
    }

    private companion object {
        const val ACTION = "dev.screengoated.toolbox.mobile.debug.PHONE_CONTROL_PROBE"
        const val CANCEL_ACTION =
            "dev.screengoated.toolbox.mobile.debug.PHONE_CONTROL_PROBE_CANCEL"
        const val EXTRA_REQUEST_ID = "request_id"
        const val EXTRA_TOOL = "tool"
        const val EXTRA_ARGUMENTS_BASE64 = "arguments_b64"
        const val EXTRA_ALLOW_MUTATION = "allow_mutation"
        const val MAX_ARGUMENT_BYTES = 64 * 1024
        const val MAX_ERROR_CHARS = 400
        const val PROBE_EXECUTION_TIMEOUT_MS = 8_000L
        val REQUEST_ID = Regex("[A-Za-z0-9_-]{1,64}")
        val JSON = Json { ignoreUnknownKeys = false }
        val PROBE_SCOPE = CoroutineScope(SupervisorJob() + Dispatchers.IO)
        val PROBE_ADMISSION = DebugProbeAdmission()
        val PROBE_DISPATCHER = DebugProbeDispatcherStore<PhoneControlToolDispatcher>()
    }
}

internal class DebugProbeDispatcherStore<T : Any> {
    @Volatile
    private var instance: T? = null

    fun getOrCreate(factory: () -> T): T = instance ?: synchronized(this) {
        instance ?: factory().also { instance = it }
    }
}

private fun probeFailure(
    errorName: String,
    message: String,
    effectMayHaveOccurred: Boolean,
): JsonObject = buildJsonObject {
    put("probe_error", errorName)
    put("message", message)
    put("effect_may_have_occurred", effectMayHaveOccurred)
    put("effect_verified", false)
    put("fresh_observation_required", effectMayHaveOccurred)
}

private fun writeBusyReceipt(context: Context, requestId: String) {
    synchronized(RECEIPT_LOCK) {
        val directory = probeOutputDirectory(context)
        val target = File(directory, "$requestId.json")
        val temporary = File(directory, ".$requestId.tmp")
        scheduleProbeReceiptExpiry(context, requestId)
        temporary.writeText(
            probeFailure(
                errorName = "ProbeBusy",
                message = "another debug probe is still in flight",
                effectMayHaveOccurred = false,
            ).toString(),
            Charsets.UTF_8,
        )
        check(temporary.renameTo(target)) { "debug probe busy result rename failed" }
    }
}

/** Non-exported alarm target that removes an abandoned debug receipt after process death. */
class PhoneControlDebugProbeCleanupReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val requestId = intent.getStringExtra(EXTRA_REQUEST_ID).orEmpty()
        if (REQUEST_ID.matches(requestId)) deleteProbeReceipt(context.applicationContext, requestId)
    }
}

private fun scheduleProbeReceiptExpiry(context: Context, requestId: String) {
    val alarm = context.getSystemService(AlarmManager::class.java) ?: return
    alarm.set(
        AlarmManager.ELAPSED_REALTIME_WAKEUP,
        SystemClock.elapsedRealtime() + RECEIPT_TTL_MS,
        requireNotNull(
            cleanupPendingIntent(context, requestId, PendingIntent.FLAG_UPDATE_CURRENT),
        ),
    )
}

private fun deleteProbeReceipt(context: Context, requestId: String) {
    synchronized(RECEIPT_LOCK) {
        probeOutputDirectory(context).let { directory ->
            File(directory, "$requestId.json").delete()
            File(directory, ".$requestId.tmp").delete()
        }
        val alarm = context.getSystemService(AlarmManager::class.java)
        val pending = cleanupPendingIntent(context, requestId, PendingIntent.FLAG_NO_CREATE)
        if (pending != null) {
            alarm?.cancel(pending)
            pending.cancel()
        }
    }
}

private fun pruneExpiredProbeReceipts(context: Context) {
    val cutoff = System.currentTimeMillis() - RECEIPT_TTL_MS
    synchronized(RECEIPT_LOCK) {
        probeOutputDirectory(context).listFiles().orEmpty().forEach { file ->
            if (file.isFile && file.lastModified() in 1 until cutoff) file.delete()
        }
    }
}

private fun probeOutputDirectory(context: Context): File =
    File(context.noBackupFilesDir, OUTPUT_DIRECTORY).apply { mkdirs() }

private fun cleanupPendingIntent(
    context: Context,
    requestId: String,
    creationFlag: Int,
): PendingIntent? {
    val intent = Intent(context, PhoneControlDebugProbeCleanupReceiver::class.java).apply {
        action = CLEANUP_ACTION
        data = Uri.parse("sgt-phone-control-probe://receipt/$requestId")
        putExtra(EXTRA_REQUEST_ID, requestId)
    }
    return PendingIntent.getBroadcast(
        context,
        0,
        intent,
        creationFlag or PendingIntent.FLAG_IMMUTABLE,
    )
}

private const val EXTRA_REQUEST_ID = "request_id"
private const val OUTPUT_DIRECTORY = "phone-control-probes"
private const val CLEANUP_ACTION = "dev.screengoated.toolbox.mobile.debug.PHONE_CONTROL_PROBE_CLEANUP"
private const val RECEIPT_TTL_MS = 5 * 60 * 1_000L
private val REQUEST_ID = Regex("[A-Za-z0-9_-]{1,64}")
private val RECEIPT_LOCK = Any()

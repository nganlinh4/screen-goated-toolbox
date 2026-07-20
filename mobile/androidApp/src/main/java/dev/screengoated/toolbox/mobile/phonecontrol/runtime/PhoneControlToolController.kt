package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicBoolean

internal const val PHONE_CONTROL_COMPLETION_QUEUE_CAPACITY = 1

internal data class PhoneControlToolCompletionEvent(
    val id: String,
    val token: Long,
    val result: PhoneControlToolExecutionResult,
)

internal data class PhoneControlCompletedTool(
    val request: PhoneControlToolRequest,
    val result: PhoneControlToolExecutionResult,
)

internal enum class PhoneControlToolAdmission {
    ACCEPTED,
    DUPLICATE_ID,
    TOOL_CALL_IN_FLIGHT,
}

internal class PhoneControlToolController(
    private val executor: PhoneControlToolExecutor,
    private val scope: CoroutineScope,
    private val completions: Channel<PhoneControlToolCompletionEvent>,
) {
    private data class Pending(
        val token: Long,
        val request: PhoneControlToolRequest,
        var started: Boolean = false,
        var job: PhoneControlToolJob? = null,
        var state: State = State.ACTIVE,
        val terminalQueued: AtomicBoolean = AtomicBoolean(false),
    ) {
        enum class State {
            ACTIVE,
            CANCELLING,
        }
    }

    private val lock = Any()
    private val tokens = AtomicLong(0L)
    private var pending: Pending? = null

    val pendingCount: Int
        get() = synchronized(lock) { if (pending == null) 0 else 1 }

    val activeRequest: PhoneControlToolRequest?
        get() = synchronized(lock) { pending?.request }

    fun dispatch(request: PhoneControlToolRequest): PhoneControlToolAdmission {
        val token = tokens.updateAndGet { current -> if (current == Long.MAX_VALUE) 1L else current + 1L }
        val record = Pending(token = token, request = request)
        synchronized(lock) {
            pending?.let { active ->
                return if (active.request.id == request.id) {
                    PhoneControlToolAdmission.DUPLICATE_ID
                } else {
                    PhoneControlToolAdmission.TOOL_CALL_IN_FLIGHT
                }
            }
            pending = record
        }
        scope.launch {
            val claimed = synchronized(lock) {
                pending
                    ?.takeIf { it.token == token && it.state == Pending.State.ACTIVE }
                    ?.also { it.started = true } != null
            }
            if (!claimed) return@launch
            val job = try {
                executor.execute(request) { result ->
                    queueTerminal(record, result)
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                Log.e(TAG, "tool_dispatch_failed id=${request.id} name=${request.name}", error)
                queueTerminal(
                    record,
                    PhoneControlToolExecutionResult(
                        response = buildJsonObject {
                            put("code", "tool_dispatch_failed")
                            put("message", error.message ?: "The tool dispatcher failed.")
                            put("effect", "proven_no_effect")
                        },
                        certainty = PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
                    ),
                )
                null
            }
            bindJob(request.id, token, job)
        }
        return PhoneControlToolAdmission.ACCEPTED
    }

    fun takeCompletion(event: PhoneControlToolCompletionEvent): PhoneControlCompletedTool? {
        val record = synchronized(lock) {
            pending
                ?.takeIf { it.request.id == event.id && it.token == event.token }
                ?.also { pending = null }
        } ?: return null
        return PhoneControlCompletedTool(record.request, event.result)
    }

    fun cancel(ids: Collection<String>): List<PhoneControlEffectCertainty> {
        val cancellation = synchronized(lock) {
            pending?.takeIf {
                it.request.id in ids && it.state == Pending.State.ACTIVE
            }?.also { it.state = Pending.State.CANCELLING }
                ?.let { record -> record to !record.started }
        } ?: return emptyList()
        val (record, cancelledBeforeStart) = cancellation
        val certainty = runCatching {
            when {
                record.job != null -> requireNotNull(record.job).cancel()
                record.started -> PhoneControlEffectCertainty.UNKNOWN
                else -> PhoneControlEffectCertainty.PROVEN_NO_EFFECT
            }
        }.getOrDefault(PhoneControlEffectCertainty.UNKNOWN)
        if (cancelledBeforeStart) queuePreDispatchCancellationReceipt(record)
        return listOf(certainty)
    }

    fun cancelAll(): List<PhoneControlEffectCertainty> {
        val ids = synchronized(lock) { listOfNotNull(pending?.request?.id) }
        return cancel(ids)
    }

    private fun bindJob(
        id: String,
        token: Long,
        job: PhoneControlToolJob?,
    ) {
        if (job == null) return
        val state = synchronized(lock) {
            pending
                ?.takeIf { it.request.id == id && it.token == token }
                ?.also { it.job = job }
                ?.state
        }
        if (state != Pending.State.ACTIVE) runCatching { job.cancel() }
    }

    private fun queuePreDispatchCancellationReceipt(record: Pending) {
        queueTerminal(
            record,
            PhoneControlToolExecutionResult(
                response = buildJsonObject {
                    put("code", "tool_cancelled")
                    put("message", "The tool job was cancelled before dispatch.")
                    put("effect", "proven_no_effect")
                },
                certainty = PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
            ),
        )
    }

    private fun queueTerminal(record: Pending, result: PhoneControlToolExecutionResult) {
        if (!record.terminalQueued.compareAndSet(false, true)) {
            Log.w(TAG, "duplicate_tool_terminal_absorbed id=${record.request.id}")
            return
        }
        val queued = completions.trySend(
            PhoneControlToolCompletionEvent(record.request.id, record.token, result),
        )
        if (queued.isFailure) {
            Log.e(TAG, "tool_completion_queue_closed id=${record.request.id}")
        }
    }

    private companion object {
        const val TAG = "SGTPhoneControlTools"
    }
}

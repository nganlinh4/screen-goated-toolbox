package dev.screengoated.toolbox.mobile.creation.worker

import android.app.Service
import android.content.Intent
import android.os.IBinder
import android.os.Process
import android.util.Log
import dev.screengoated.toolbox.mobile.creation.CreationTool
import dev.screengoated.toolbox.mobile.creation.CreationContract
import dev.screengoated.toolbox.mobile.creation.CreationWorkerEvent
import dev.screengoated.toolbox.mobile.creation.CreationWorkerRequest
import dev.screengoated.toolbox.mobile.creation.creationWorkerCanServeFollowUp
import dev.screengoated.toolbox.mobile.creation.decodeCreationWorkerEvent
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeEngine
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeEventSink
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeManager
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.Json

internal abstract class CreationWorkerService : Service() {
    protected abstract val workerTool: CreationTool
    protected abstract val executionIndex: Int
    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
        explicitNulls = false
    }
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val jobs = ConcurrentHashMap<String, Job>()
    private var engine: CreationRuntimeEngine? = null
    private val runtime by lazy { CreationRuntimeManager.get(this) }
    private val binder = object : ICreationWorker.Stub() {
        override fun prepare(callback: ICreationWorkerCallback) {
            scope.launch {
                val activeEngine = runCatching { engine() }.getOrElse {
                    callback.emit(
                        CreationWorkerEvent(
                            event = "failure",
                            ready = false,
                            failureCode = "runtime_unavailable",
                        ),
                    )
                    return@launch
                }
                var terminalEmitted = false
                var prepared = false
                runCatching {
                    activeEngine.prepare(
                        eventSink(callback) { event ->
                            if (event.event == "ready" || event.event == "failure") {
                                terminalEmitted = true
                            }
                            if (event.event == "ready" && event.ready == true) {
                                prepared = true
                            }
                        },
                    )
                    check(terminalEmitted) { "Creation preparation returned no terminal event" }
                    check(prepared) { "Creation preparation did not make the engine ready" }
                }
                    .onFailure {
                        activeEngine.destroy()
                        if (engine === activeEngine) engine = null
                        callback.emit(
                            CreationWorkerEvent(
                                event = "failure",
                                ready = false,
                                failureCode = "runtime_unavailable",
                            ),
                        )
                    }
            }
        }

        override fun supportsRequest(requestJson: String): Boolean =
            decodeSupportedRequest(requestJson) != null

        override fun runJob(requestJson: String, callback: ICreationWorkerCallback) {
            val request = runCatching {
                json.decodeFromString(CreationWorkerRequest.serializer(), requestJson)
            }.getOrElse {
                callback.emit(CreationWorkerEvent(event = "failure", failureCode = "input"))
                return
            }
            val activeEngine = runCatching { engine() }.getOrElse {
                callback.emit(
                    CreationWorkerEvent(
                        jobId = request.jobId,
                        event = "failure",
                        failureCode = "runtime_unavailable",
                    ),
                )
                return
            }
            if (!creationWorkerStructurallySupports(workerTool, executionIndex, request)) {
                callback.emit(
                    CreationWorkerEvent(
                        jobId = request.jobId,
                        event = "failure",
                        failureCode = "unsupported",
                    ),
                )
                return
            }
            jobs.remove(request.jobId)?.cancel()
            jobs[request.jobId] = scope.launch {
                val terminal = CreationWorkerTerminalRelay { callback.emit(it) }
                try {
                    val remainingMs = request.deadlineAtMs - System.currentTimeMillis()
                    if (remainingMs <= 0L) {
                        terminal.accept(
                            CreationWorkerEvent(
                                jobId = request.jobId,
                                generationMode = request.generationMode,
                                event = "failure",
                                failureCode = "timeout",
                            ),
                        )
                        return@launch
                    }
                    Log.i(
                        "CreationWorker",
                        "job_started tool=${workerTool.wireName} slot=$executionIndex " +
                            "dispatchId=${request.dispatchId} " +
                            "requestFingerprint=${request.requestFingerprint}",
                    )
                    withTimeout(remainingMs) {
                        activeEngine.runJob(
                            requestJson,
                            decodedEventSink(terminal::accept),
                        )
                    }
                } catch (_: TimeoutCancellationException) {
                    terminal.accept(
                        CreationWorkerEvent(
                            jobId = request.jobId,
                            generationMode = request.generationMode,
                            event = "failure",
                            failureCode = "timeout",
                        ),
                    )
                } catch (_: CancellationException) {
                    terminal.accept(
                        CreationWorkerEvent(
                            jobId = request.jobId,
                            generationMode = request.generationMode,
                            event = "cancelled",
                        ),
                    )
                } catch (_: Throwable) {
                    terminal.accept(
                        CreationWorkerEvent(
                            jobId = request.jobId,
                            generationMode = request.generationMode,
                            event = "failure",
                            failureCode = "unexpected",
                        ),
                    )
                } finally {
                    jobs.remove(request.jobId)
                    val completed = terminal.complete(
                        CreationWorkerEvent(
                            jobId = request.jobId,
                            generationMode = request.generationMode,
                            event = "failure",
                            failureCode = "unexpected",
                        ),
                    )
                    val followUpReady = creationWorkerCanServeFollowUp(completed)
                    Log.i(
                        "CreationWorker",
                        "job_terminal tool=${workerTool.wireName} slot=$executionIndex " +
                            "followUpReady=$followUpReady",
                    )
                    if (!followUpReady) {
                        runCatching(activeEngine::destroy)
                        if (engine === activeEngine) engine = null
                    }
                    callback.emit(completed)
                }
            }
        }

        override fun cancel(jobId: String) {
            jobs.remove(jobId)?.cancel()
        }
    }

    override fun onBind(intent: Intent?): IBinder = binder

    override fun onDestroy() {
        engine?.destroy()
        engine = null
        scope.cancel()
        super.onDestroy()
        Process.killProcess(Process.myPid())
    }

    private fun engine(): CreationRuntimeEngine = engine ?: run {
        val factory = runtime.factory() ?: error("Creation runtime is not installed")
        factory.createEngine(this, workerTool.wireName, executionIndex).also { engine = it }
    }

    private fun decodeSupportedRequest(requestJson: String): CreationWorkerRequest? =
        runCatching {
            json.decodeFromString(CreationWorkerRequest.serializer(), requestJson)
        }.getOrNull()?.takeIf {
            creationWorkerStructurallySupports(workerTool, executionIndex, it)
        }

    private fun eventSink(
        callback: ICreationWorkerCallback,
        observe: (CreationWorkerEvent) -> Unit,
    ) = decodedEventSink { event ->
        observe(event)
        callback.emit(event)
    }

    private fun decodedEventSink(
        receive: (CreationWorkerEvent) -> Unit,
    ) = CreationRuntimeEventSink { eventJson ->
        val event = decodeCreationWorkerEvent(eventJson) ?: return@CreationRuntimeEventSink
        receive(event)
    }

    private fun ICreationWorkerCallback.emit(event: CreationWorkerEvent) {
        runCatching { onEvent(json.encodeToString(CreationWorkerEvent.serializer(), event)) }
            .onSuccess {
                if (event.event == "ready" || event.event == "failure") {
                    Log.i("CreationWorker", "preparation_terminal_sent event=${event.event}")
                }
            }
            .onFailure {
                Log.w("CreationWorker", "worker_event_delivery_failed", it)
            }
    }

}

internal class ImageTo3dWorker0Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_TO_3D
    override val executionIndex = 0
}

internal class ImageTo3dWorker1Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_TO_3D
    override val executionIndex = 1
}

internal class ImageToSvgWorker0Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_TO_SVG
    override val executionIndex = 0
}

internal class ImageToSvgWorker1Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_TO_SVG
    override val executionIndex = 1
}

internal class ImageCreatorWorker0Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_CREATOR
    override val executionIndex = 0
}

internal class ImageCreatorWorker1Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_CREATOR
    override val executionIndex = 1
}

package dev.screengoated.toolbox.mobile.creation.worker

import android.app.Service
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.os.IBinder
import android.util.Log
import dev.screengoated.toolbox.mobile.creation.CreationDiagnostics
import dev.screengoated.toolbox.mobile.creation.CreationTool
import dev.screengoated.toolbox.mobile.creation.CreationWorkerEvent
import dev.screengoated.toolbox.mobile.creation.CreationWorkerRequest
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json

internal abstract class CreationWorkerService : Service() {
    protected abstract val workerTool: CreationTool
    protected abstract val workerSlot: Int
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val jobs = ConcurrentHashMap<String, Job>()
    private var engine: CreationAutomationEngine? = null
    private val diagnostics by lazy {
        CreationDiagnostics(this, "worker-${workerTool.wireName}-$workerSlot")
    }

    private val binder = object : ICreationWorker.Stub() {
        override fun prepare(callback: ICreationWorkerCallback) {
            scope.launch {
                val activeEngine = engine()
                diagnostics.event("prepare_started", workerTool.wireName, workerSlot)
                runCatching {
                    activeEngine.prepare { event ->
                        diagnostics.event(
                            name = if (event.event == "ready") "prepare_ready" else "prepare_progress",
                            tool = workerTool.wireName,
                            slot = workerSlot,
                            stage = event.progressKey ?: event.stage,
                        )
                        callback.emit(event)
                    }
                }
                    .onFailure {
                        if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0) {
                            Log.e(DEBUG_TAG, "Preparation failed for ${workerTool.wireName}-$workerSlot", it)
                        }
                        activeEngine.destroy()
                        if (engine === activeEngine) engine = null
                        diagnostics.event(
                            "prepare_failed",
                            workerTool.wireName,
                            workerSlot,
                            failure = it,
                        )
                        callback.emit(
                            CreationWorkerEvent(
                                event = "failure",
                                ready = false,
                                error = it.message ?: "Workspace preparation failed",
                            ),
                        )
                    }
            }
        }

        override fun runJob(requestJson: String, callback: ICreationWorkerCallback) {
            val request = runCatching {
                json.decodeFromString(CreationWorkerRequest.serializer(), requestJson)
            }.getOrElse {
                callback.emit(CreationWorkerEvent(event = "failure", error = "Invalid job request"))
                return
            }
            if (request.tool != workerTool.wireName) {
                callback.emit(
                    CreationWorkerEvent(
                        jobId = request.jobId,
                        event = "failure",
                        error = "Job was routed to the wrong worker",
                    ),
                )
                return
            }
            jobs.remove(request.jobId)?.cancel()
            jobs[request.jobId] = scope.launch {
                val activeEngine = engine()
                diagnostics.event(
                    "job_started",
                    workerTool.wireName,
                    workerSlot,
                    request.jobId,
                    request.operation,
                )
                var lastStage: String? = null
                try {
                    activeEngine.run(request) { event ->
                        val stage = event.progressKey ?: event.stage ?: event.event
                        if (stage != lastStage) {
                            lastStage = stage
                            diagnostics.event(
                                "job_progress",
                                workerTool.wireName,
                                workerSlot,
                                request.jobId,
                                stage,
                            )
                        }
                        callback.emit(event)
                    }
                } catch (error: TimeoutCancellationException) {
                    if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0) {
                        Log.e(DEBUG_TAG, "Job timed out for ${workerTool.wireName}-$workerSlot", error)
                    }
                    diagnostics.event(
                        "job_failed",
                        workerTool.wireName,
                        workerSlot,
                        request.jobId,
                        lastStage,
                        error,
                    )
                    callback.emit(
                        CreationWorkerEvent(
                            jobId = request.jobId,
                            event = "failure",
                            error = error.message ?: "Creation timed out",
                        ),
                    )
                } catch (_: CancellationException) {
                    diagnostics.event(
                        "job_cancelled",
                        workerTool.wireName,
                        workerSlot,
                        request.jobId,
                    )
                    callback.emit(CreationWorkerEvent(jobId = request.jobId, event = "cancelled"))
                } catch (error: Throwable) {
                    if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0) {
                        Log.e(DEBUG_TAG, "Job failed for ${workerTool.wireName}-$workerSlot", error)
                    }
                    diagnostics.event(
                        "job_failed",
                        workerTool.wireName,
                        workerSlot,
                        request.jobId,
                        lastStage,
                        error,
                    )
                    callback.emit(
                        CreationWorkerEvent(
                            jobId = request.jobId,
                            event = "failure",
                            error = error.message ?: "Creation failed",
                        ),
                    )
                } finally {
                    jobs.remove(request.jobId)
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
    }

    private fun engine(): CreationAutomationEngine = engine
        ?: CreationAutomationEngine(this, workerTool, workerSlot).also { engine = it }

    private fun ICreationWorkerCallback.emit(event: CreationWorkerEvent) {
        runCatching { onEvent(json.encodeToString(CreationWorkerEvent.serializer(), event)) }
    }

    private companion object {
        const val DEBUG_TAG = "CreationRuntimeDebug"
    }
}

internal class ImageTo3dWorker0Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_TO_3D
    override val workerSlot = 0
}

internal class ImageTo3dWorker1Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_TO_3D
    override val workerSlot = 1
}

internal class ImageTo3dWorker2Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_TO_3D
    override val workerSlot = 2
}

internal class ImageTo3dWorker3Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_TO_3D
    override val workerSlot = 3
}

internal class ImageToSvgWorker0Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_TO_SVG
    override val workerSlot = 0
}

internal class ImageToSvgWorker1Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_TO_SVG
    override val workerSlot = 1
}

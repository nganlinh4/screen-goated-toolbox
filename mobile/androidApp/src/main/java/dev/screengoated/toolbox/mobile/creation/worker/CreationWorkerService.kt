package dev.screengoated.toolbox.mobile.creation.worker

import android.app.Service
import android.content.Intent
import android.os.IBinder
import dev.screengoated.toolbox.mobile.creation.CreationTool
import dev.screengoated.toolbox.mobile.creation.CreationWorkerEvent
import dev.screengoated.toolbox.mobile.creation.CreationWorkerRequest
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json

internal abstract class CreationWorkerService : Service() {
    protected abstract val workerTool: CreationTool
    protected abstract val workerSlot: Int
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val jobs = ConcurrentHashMap<String, Job>()
    private val engine by lazy { CreationAutomationEngine(this, workerTool, workerSlot) }

    private val binder = object : ICreationWorker.Stub() {
        override fun prepare(callback: ICreationWorkerCallback) {
            scope.launch {
                runCatching { engine.prepare { callback.emit(it) } }
                    .onFailure {
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
                try {
                    engine.run(request) { callback.emit(it) }
                } catch (_: CancellationException) {
                    callback.emit(CreationWorkerEvent(jobId = request.jobId, event = "cancelled"))
                } catch (error: Throwable) {
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
        engine.destroy()
        scope.cancel()
        super.onDestroy()
    }

    private fun ICreationWorkerCallback.emit(event: CreationWorkerEvent) {
        runCatching { onEvent(json.encodeToString(CreationWorkerEvent.serializer(), event)) }
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

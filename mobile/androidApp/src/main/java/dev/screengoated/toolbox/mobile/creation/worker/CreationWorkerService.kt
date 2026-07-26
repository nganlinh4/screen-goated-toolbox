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
import dev.screengoated.toolbox.mobile.creation.publicImageCreationFailure
import dev.screengoated.toolbox.mobile.creation.publicImageCreationStage
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
import kotlinx.serialization.json.Json

internal abstract class CreationWorkerService : Service() {
    protected abstract val workerTool: CreationTool
    protected abstract val workerSlot: Int
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val jobs = ConcurrentHashMap<String, Job>()
    private var engine: CreationRuntimeEngine? = null
    private val runtime by lazy { CreationRuntimeManager.get(this) }
    private val diagnostics by lazy {
        CreationDiagnostics(this, "worker-${workerTool.wireName}-$workerSlot")
    }

    private val binder = object : ICreationWorker.Stub() {
        override fun prepare(callback: ICreationWorkerCallback) {
            scope.launch {
                val activeEngine = engine()
                diagnostics.event("prepare_started", workerTool.wireName, workerSlot)
                runCatching {
                    activeEngine.prepare(eventSink(callback) { event ->
                        diagnostics.event(
                            name = if (event.event == "ready") "prepare_ready" else "prepare_progress",
                            tool = workerTool.wireName,
                            slot = workerSlot,
                            stage = diagnosticStage(event),
                            provider = event.provider,
                        )
                    })
                }
                    .onFailure {
                        logFailure("Preparation failed", it)
                        activeEngine.destroy()
                        if (engine === activeEngine) engine = null
                        diagnostics.event(
                            "prepare_failed",
                            workerTool.wireName,
                            workerSlot,
                            failure = it,
                            failureCategoryOverride = diagnosticFailureCategory(),
                        )
                        callback.emit(
                            CreationWorkerEvent(
                                event = "failure",
                                ready = false,
                                error = publicFailure(it, preparation = true),
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
                        provider = request.provider,
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
                    generationMode = request.generationMode,
                    provider = request.provider,
                )
                var lastStage: String? = null
                try {
                    activeEngine.runJob(requestJson, eventSink(callback, request.provider) { event ->
                        val stage = diagnosticStage(event)
                        if (stage != lastStage) {
                            lastStage = stage
                            diagnostics.event(
                                "job_progress",
                                workerTool.wireName,
                                workerSlot,
                                request.jobId,
                                stage,
                                generationMode = request.generationMode,
                                provider = request.provider,
                            )
                        }
                    })
                } catch (error: TimeoutCancellationException) {
                    logFailure("Job timed out", error)
                    diagnostics.event(
                        "job_failed",
                        workerTool.wireName,
                        workerSlot,
                        request.jobId,
                        lastStage,
                        error,
                        generationMode = request.generationMode,
                        provider = request.provider,
                        failureCategoryOverride = diagnosticFailureCategory(),
                    )
                    callback.emit(
                        CreationWorkerEvent(
                            jobId = request.jobId,
                            generationMode = request.generationMode,
                            provider = request.provider,
                            event = "failure",
                            error = publicFailure(error),
                        ),
                    )
                } catch (_: CancellationException) {
                    diagnostics.event(
                        "job_cancelled",
                        workerTool.wireName,
                        workerSlot,
                        request.jobId,
                        generationMode = request.generationMode,
                        provider = request.provider,
                    )
                    callback.emit(
                        CreationWorkerEvent(
                            jobId = request.jobId,
                            generationMode = request.generationMode,
                            provider = request.provider,
                            event = "cancelled",
                        ),
                    )
                } catch (error: Throwable) {
                    logFailure("Job failed", error)
                    diagnostics.event(
                        "job_failed",
                        workerTool.wireName,
                        workerSlot,
                        request.jobId,
                        lastStage,
                        error,
                        generationMode = request.generationMode,
                        provider = request.provider,
                        failureCategoryOverride = diagnosticFailureCategory(),
                    )
                    callback.emit(
                        CreationWorkerEvent(
                            jobId = request.jobId,
                            generationMode = request.generationMode,
                            provider = request.provider,
                            event = "failure",
                            error = publicFailure(error),
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

    private fun publicFailure(error: Throwable, preparation: Boolean = false): String {
        if (workerTool != CreationTool.IMAGE_CREATOR) {
            return error.message ?: if (preparation) {
                "Workspace preparation failed"
            } else {
                "Creation failed"
            }
        }
        return if (preparation) {
            "Image creation is not ready yet. Retry later."
        } else {
            publicImageCreationFailure()
        }
    }

    private fun logFailure(label: String, error: Throwable) {
        if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE == 0) return
        val context = "$label for ${workerTool.wireName}-$workerSlot"
        if (workerTool == CreationTool.IMAGE_CREATOR) {
            Log.e(DEBUG_TAG, "$context category=image_creation")
        } else {
            Log.e(DEBUG_TAG, context, error)
        }
    }

    private fun diagnosticStage(event: CreationWorkerEvent): String {
        if (workerTool != CreationTool.IMAGE_CREATOR) {
            return event.progressKey ?: event.stage ?: event.event
        }
        return "image.${publicImageCreationStage(event.stage ?: event.event)}"
    }

    private fun diagnosticFailureCategory(): String? =
        IMAGE_CREATION_FAILURE_CATEGORY.takeIf { workerTool == CreationTool.IMAGE_CREATOR }

    override fun onDestroy() {
        engine?.destroy()
        engine = null
        scope.cancel()
        super.onDestroy()
    }

    private fun engine(): CreationRuntimeEngine = engine ?: run {
        val factory = runtime.factory() ?: error("Creation runtime is not installed")
        factory.createAutomation(this, workerTool.wireName, workerSlot).also { engine = it }
    }

    private fun eventSink(
        callback: ICreationWorkerCallback,
        provider: String? = null,
        observe: (CreationWorkerEvent) -> Unit,
    ) = CreationRuntimeEventSink { eventJson ->
        val event = runCatching {
            json.decodeFromString(CreationWorkerEvent.serializer(), eventJson)
        }.getOrNull() ?: return@CreationRuntimeEventSink
        val normalized = if (provider != null && event.provider == null) {
            event.copy(provider = provider)
        } else {
            event
        }
        observe(normalized)
        val outbound = if (normalized === event) {
            eventJson
        } else {
            json.encodeToString(CreationWorkerEvent.serializer(), normalized)
        }
        runCatching { callback.onEvent(outbound) }
    }

    private fun ICreationWorkerCallback.emit(event: CreationWorkerEvent) {
        runCatching { onEvent(json.encodeToString(CreationWorkerEvent.serializer(), event)) }
    }

    private companion object {
        const val DEBUG_TAG = "CreationRuntimeDebug"
        private const val IMAGE_CREATION_FAILURE_CATEGORY = "image_creation"
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

internal class ImageCreatorWorker0Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_CREATOR
    override val workerSlot = 0
}

internal class ImageCreatorWorker1Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_CREATOR
    override val workerSlot = 1
}

internal class ImageCreatorWorker2Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_CREATOR
    override val workerSlot = 2
}

internal class ImageCreatorWorker3Service : CreationWorkerService() {
    override val workerTool = CreationTool.IMAGE_CREATOR
    override val workerSlot = 3
}

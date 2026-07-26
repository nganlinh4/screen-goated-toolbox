package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.graphics.BitmapFactory
import java.io.File
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class CreationJobManager private constructor(context: Context) {
    val files = CreationFileStore(context)
    val history = CreationHistoryStore(context, files)
    private val diagnostics = CreationDiagnostics(context, "manager")
    private val workers = CreationWorkerPool.get(context)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val lock = Any()
    private val jobs = linkedMapOf<String, CreationJobStatus>()
    private val requests = mutableMapOf<String, CreationWorkerRequest>()
    private val startedAt = mutableMapOf<String, Long>()
    private val continuations = mutableMapOf<String, Continuation>()
    private val workerKeys = mutableMapOf<String, String>()

    fun startPreparation(priority: CreationTool? = null): String {
        workers.startPreparation(priority)
        return "preparing"
    }

    fun preparationStatus(tool: CreationTool): String = workers.preparationStatus(tool)

    fun removeRuntime() = workers.removeRuntime()

    fun startJob(tool: CreationTool, args: JsonObject): CreationJobStatus {
        synchronized(lock) {
            require(runningCount(tool) < CreationContract.maximumParallelJobs(tool)) {
                if (tool == CreationTool.IMAGE_CREATOR) {
                    "Two images are already being created"
                } else {
                    "Both creation workers are busy"
                }
            }
        }
        val jobId = nextJobId(tool)
        val draft = CreationJobFactory.create(tool, args, files, jobId)
        val request = draft.request
        val status = draft.status
        synchronized(lock) {
            jobs[jobId] = status
            requests[jobId] = request
            startedAt[jobId] = System.currentTimeMillis()
        }
        diagnostics.event(
            "job_queued",
            tool.wireName,
            jobId = jobId,
            stage = "preparing",
            generationMode = request.generationMode,
            provider = request.provider,
        )
        dispatchWhenAvailable(request)
        return status
    }

    fun startSegmentation(continuationId: String): CreationJobStatus {
        val continuation = synchronized(lock) {
            require(runningCount(CreationTool.IMAGE_TO_3D) < CreationContract.MAXIMUM_PARALLEL_JOBS) {
                "Both creation workers are busy"
            }
            continuations.remove(continuationId)
                ?: error("This model can no longer be separated into parts")
        }
        check(continuation.provider == CreationProvider.TRIPO.wireName) {
            "This result cannot be separated after generation"
        }
        val jobId = nextJobId(CreationTool.IMAGE_TO_3D)
        val output = files.stagingFile(CreationTool.IMAGE_TO_3D, continuation.sourcePath, "glb")
        val request = CreationWorkerRequest(
            jobId = jobId,
            tool = CreationTool.IMAGE_TO_3D.wireName,
            generationMode = CreationGenerationMode.QUALITY.wireName,
            provider = continuation.provider,
            operation = "segment",
            imagePath = continuation.sourcePath,
            outputPath = output.absolutePath,
            outputName = output.name,
            autoSegment = true,
            continuationToken = continuation.token,
            previousOutputPath = continuation.outputPath,
        )
        val status = CreationJobFactory.initialStatus(CreationTool.IMAGE_TO_3D, request).copy(
            stage = "segmenting",
            progressText = "Separating model parts.",
            phase = "separation",
            outputPath = continuation.outputPath,
            outputName = continuation.outputName,
        )
        synchronized(lock) {
            val affected = jobs.values.filter { current ->
                current.jobId != null && continuations[current.jobId]?.workerKey == continuation.workerKey
            }.mapNotNull(CreationJobStatus::jobId)
            affected.forEach { affectedId ->
                jobs[affectedId]?.let { jobs[affectedId] = it.copy(canSegment = false) }
            }
            continuations.entries.removeAll { it.value.workerKey == continuation.workerKey }
            jobs[jobId] = status
            requests[jobId] = request
            startedAt[jobId] = System.currentTimeMillis()
        }
        diagnostics.event(
            "job_queued",
            CreationTool.IMAGE_TO_3D.wireName,
            jobId = jobId,
            stage = "segmenting",
            generationMode = request.generationMode,
            provider = request.provider,
        )
        dispatchWhenAvailable(request, continuation.workerKey)
        return status
    }

    fun status(tool: CreationTool, jobId: String?): CreationJobStatus = synchronized(lock) {
        val current = jobId?.let(jobs::get)
            ?: jobs.values.lastOrNull { requestTool(it.jobId) == tool }
            ?: CreationJobFactory.idleStatus(tool)
        withElapsed(current)
    }

    fun statuses(tool: CreationTool): List<CreationJobStatus> = synchronized(lock) {
        jobs.values.filter { requestTool(it.jobId) == tool }.map(::withElapsed)
    }

    fun cancel(tool: CreationTool, jobId: String?): List<CreationJobStatus> {
        val targets = synchronized(lock) {
            val ids = if (jobId != null) listOf(jobId) else jobs.values
                .filter { requestTool(it.jobId) == tool && creationStageIsBusy(it.stage) }
                .mapNotNull { it.jobId }
            val transitioned = mutableListOf<String>()
            ids.forEach { id ->
                jobs[id]?.takeIf { creationStageIsBusy(it.stage) }?.let {
                    jobs[id] = it.copy(stage = "cancelled", progressText = "Cancelled.")
                    transitioned += id
                }
                requests[id]?.outputPath?.let(::File).let { file -> if (file?.length() == 0L) file.delete() }
            }
            transitioned
        }
        targets.forEach(workers::cancel)
        return statuses(tool)
    }

    fun renameHistory(tool: CreationTool, id: String, name: String): CreationHistoryEntry {
        val previous = history.list(tool).firstOrNull { it.id == id }
            ?: error("Result is no longer in history")
        val updated = history.rename(id, name)
        synchronized(lock) {
            jobs.replaceAll { _, status ->
                if (status.outputPath == previous.outputPath) {
                    status.copy(outputPath = updated.outputPath, outputName = updated.outputName)
                } else status
            }
            continuations.replaceAll { _, value ->
                if (value.outputPath == previous.outputPath) {
                    value.copy(outputPath = updated.outputPath, outputName = updated.outputName)
                } else value
            }
        }
        return updated
    }

    fun deleteHistory(tool: CreationTool, id: String) {
        val previous = history.list(tool).firstOrNull { it.id == id }
            ?: error("Result is no longer in history")
        history.delete(id)
        synchronized(lock) {
            jobs.replaceAll { _, status ->
                if (status.outputPath == previous.outputPath) {
                    status.copy(outputPath = null, outputName = null, canSegment = false)
                } else status
            }
            continuations.entries.removeAll { it.value.outputPath == previous.outputPath }
        }
    }

    private fun dispatchWhenAvailable(request: CreationWorkerRequest, preferred: String? = null) {
        scope.launch {
            var waitingSeconds = 0
            while (true) {
                if (synchronized(lock) { jobs[request.jobId]?.stage == "cancelled" }) return@launch
                val worker = workers.dispatch(
                    request,
                    preferred,
                    ::handleWorkerEvent,
                ) { assignedWorker ->
                    synchronized(lock) {
                        workerKeys[request.jobId] = assignedWorker
                        if (request.operation == "generate") {
                            val invalidated = continuations
                                .filterValues { it.workerKey == assignedWorker }
                                .keys
                                .toSet()
                            continuations.keys.removeAll(invalidated)
                            invalidated.forEach { id ->
                                jobs[id]?.let { jobs[id] = it.copy(canSegment = false) }
                            }
                        }
                    }
                }
                if (worker != null) {
                    diagnostics.event(
                        "job_dispatched",
                        request.tool,
                        jobId = request.jobId,
                        stage = "preparing",
                        generationMode = request.generationMode,
                        provider = request.provider,
                    )
                    return@launch
                }
                waitingSeconds += 1
                if (waitingSeconds % DISPATCH_WAIT_LOG_SECONDS == 0) {
                    diagnostics.event(
                        "job_waiting_for_workspace",
                        request.tool,
                        jobId = request.jobId,
                        stage = "preparing",
                        generationMode = request.generationMode,
                        provider = request.provider,
                    )
                }
                delay(1_000)
            }
        }
    }

    private fun handleWorkerEvent(workerKey: String, event: CreationWorkerEvent) {
        val jobId = event.jobId ?: return
        scope.launch {
            val expectedRequest = synchronized(lock) { requests[jobId] }
            val expectedProvider = expectedRequest?.provider
            if (expectedRequest?.generationMode != null &&
                event.generationMode != null &&
                event.generationMode != expectedRequest.generationMode
            ) {
                fail(jobId, "Creation runtime returned a conflicting mode")
                return@launch
            }
            if (expectedProvider != null &&
                event.provider != null &&
                event.provider != expectedProvider
            ) {
                fail(jobId, "Creation runtime returned a conflicting provider")
                return@launch
            }
            when (event.event) {
                "success" -> finish(workerKey, jobId, event)
                "failure" -> {
                    val error = event.error ?: "Creation failed"
                    val publicError = if (expectedRequest?.tool == CreationTool.IMAGE_CREATOR.wireName) {
                        publicImageCreationFailure()
                    } else {
                        error
                    }
                    val route = runCatching {
                        routeCreationWorkerFailure(expectedProvider, error)
                    }.getOrElse { invalidOwner ->
                        fail(jobId, invalidOwner.message ?: "Creation failed")
                        return@launch
                    }
                    when (route) {
                        CreationWorkerFailureRoute.Fail -> fail(jobId, publicError)
                        is CreationWorkerFailureRoute.Redispatch -> {
                            val request = synchronized(lock) { requests[jobId] }
                                ?: return@launch
                            diagnostics.event(
                                "job_recovery_redirected",
                                request.tool,
                                jobId = jobId,
                                stage = route.preferredWorker,
                                generationMode = request.generationMode,
                                provider = request.provider,
                            )
                            dispatchWhenAvailable(
                                request,
                                preferred = route.preferredWorker,
                            )
                        }
                    }
                }
                "cancelled" -> cancel(requestTool(jobId) ?: return@launch, jobId)
                else -> updateProgress(jobId, event)
            }
        }
    }

    private fun updateProgress(jobId: String, event: CreationWorkerEvent) {
        var changedStage: String? = null
        val tool = synchronized(lock) {
            val current = jobs[jobId] ?: return@synchronized null
            if (current.stage == "cancelled") return@synchronized null
            val requestTool = requests[jobId]?.tool
            val isImageCreator = requestTool == CreationTool.IMAGE_CREATOR.wireName
            val observedStage = event.stage ?: current.stage
            val nextStage = if (isImageCreator) {
                publicImageCreationStage(observedStage)
            } else {
                observedStage
            }
            if (nextStage != current.stage || event.progressKey != null) {
                changedStage = if (isImageCreator) "image.$nextStage" else event.progressKey ?: nextStage
            }
            jobs[jobId] = current.copy(
                stage = nextStage,
                progressText = if (isImageCreator) {
                    publicImageCreationText(nextStage)
                } else {
                    event.progressText?.let(::publicCreationText) ?: current.progressText
                },
                phase = if (isImageCreator) nextStage else event.phase ?: current.phase,
                workspaceState = if (isImageCreator) null else {
                    event.workspaceState ?: current.workspaceState
                },
                progressRatio = event.progressRatio ?: current.progressRatio,
                estimatedTotalMs = event.estimatedTotalMs ?: current.estimatedTotalMs,
                timingSampleCount = event.timingSampleCount ?: current.timingSampleCount,
                generationMode = event.generationMode ?: current.generationMode,
                provider = event.provider ?: current.provider,
                outputPath = event.outputPath ?: current.outputPath,
                outputName = event.outputName ?: current.outputName,
                mimeType = event.mimeType ?: current.mimeType,
                width = event.width ?: current.width,
                height = event.height ?: current.height,
                isSegmented = event.isSegmented ?: current.isSegmented,
                canSegment = event.canSegment ?: current.canSegment,
                faces = event.faces ?: current.faces,
                vertices = event.vertices ?: current.vertices,
            )
            requestTool
        }
        changedStage?.let { stage ->
            diagnostics.event(
                "job_progress",
                tool,
                jobId = jobId,
                stage = stage,
                generationMode = synchronized(lock) { requests[jobId]?.generationMode },
                provider = synchronized(lock) { requests[jobId]?.provider },
            )
        }
    }

    private fun finish(workerKey: String, jobId: String, event: CreationWorkerEvent) {
        val completion = runCatching {
            synchronized(lock) {
                val request = requests[jobId] ?: return
                val current = jobs[jobId] ?: error("Creation job disappeared")
                if (!creationStageIsBusy(current.stage)) {
                    if (current.stage == "cancelled") File(request.outputPath).delete()
                    return
                }
                val staging = File(request.outputPath)
                require(
                    event.outputPath == null ||
                        File(event.outputPath).canonicalFile == staging.canonicalFile,
                ) { "Creation runtime returned a conflicting output path" }
                require(staging.isFile && staging.length() > 0L) {
                    "Creation ended without an output file"
                }
                val mime = when (request.tool) {
                    CreationTool.IMAGE_TO_3D.wireName -> "model/gltf-binary"
                    CreationTool.IMAGE_TO_SVG.wireName -> "image/svg+xml"
                    CreationTool.IMAGE_CREATOR.wireName -> {
                        require(event.mimeType == null || event.mimeType == "image/png") {
                            "Creation runtime returned an unsupported image type"
                        }
                        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                        BitmapFactory.decodeFile(staging.absolutePath, bounds)
                        require(bounds.outWidth > 0 && bounds.outHeight > 0) {
                            "Creation runtime returned an invalid image"
                        }
                        require(
                            event.width == bounds.outWidth && event.height == bounds.outHeight,
                        ) { "Creation runtime returned conflicting image dimensions" }
                        "image/png"
                    }
                    else -> error("Unsupported creation tool")
                }
                val published = files.publish(staging, request.outputName, mime)
                request.previousOutputPath?.takeIf(files::exists)?.let(files::delete)
                val segmented = request.provider == CreationProvider.MESHY.wireName ||
                    (event.isSegmented ?: request.autoSegment)
                val updated = current.copy(
                    stage = "done",
                    progressText = when (request.tool) {
                        CreationTool.IMAGE_TO_3D.wireName -> "Model ready."
                        CreationTool.IMAGE_TO_SVG.wireName -> "Vector ready"
                        else -> "Image ready"
                    },
                    phase = "complete",
                    progressRatio = 1.0,
                    outputPath = published,
                    outputName = request.outputName,
                    mimeType = mime,
                    width = event.width,
                    height = event.height,
                    generationMode = request.generationMode,
                    provider = request.provider,
                    isSegmented = segmented,
                    canSegment = request.tool == CreationTool.IMAGE_TO_3D.wireName &&
                        request.operation == "generate" &&
                        CreationContract.canContinueSegmentation(
                            request.provider,
                            segmented,
                            event.canSegment != false,
                        ),
                    faces = event.faces,
                    vertices = event.vertices,
                    error = null,
                )
                jobs[jobId] = updated
                if (updated.canSegment && event.continuationToken != null) {
                    continuations[jobId] = Continuation(
                        workerKey,
                        event.continuationToken,
                        request.imagePath,
                        published,
                        request.outputName,
                        requireNotNull(request.provider),
                    )
                }
                Completion(request, segmented, published)
            }
        }.getOrElse {
            fail(jobId, it.message ?: "Could not save creation result")
            return
        }
        val request = completion.request
        val tool = CreationTool.fromWireName(request.tool) ?: return
        val metadata = buildJsonObject {
            if (tool == CreationTool.IMAGE_TO_3D) {
                put("isSegmented", completion.segmented)
                request.generationMode?.let { put("generationMode", it) }
                request.provider?.let { put("provider", it) }
                event.faces?.let { put("faces", it) }
                event.vertices?.let { put("vertices", it) }
            } else if (tool == CreationTool.IMAGE_TO_SVG) {
                put("model", request.model)
            } else {
                put("operation", request.operation)
                put("prompt", requireNotNull(request.prompt))
                put("sourceImagePaths", JsonArray(request.imagePaths.map(::JsonPrimitive)))
                put("mimeType", "image/png")
                event.width?.let { put("width", it) }
                event.height?.let { put("height", it) }
            }
        }
        runCatching {
            history.record(
                tool,
                request.imagePath,
                completion.publishedPath,
                request.outputName,
                metadata,
            )
        }.onFailure { error ->
            diagnostics.event(
                "history_record_failed",
                request.tool,
                jobId = jobId,
                stage = "done",
                failureMessage = error.message,
                generationMode = request.generationMode,
                provider = request.provider,
            )
        }
        diagnostics.event(
            "job_succeeded",
            request.tool,
            jobId = jobId,
            stage = "done",
            generationMode = request.generationMode,
            provider = request.provider,
        )
    }

    private fun fail(jobId: String, message: String) {
        val failure = synchronized(lock) {
            val current = jobs[jobId] ?: return
            if (current.stage == "cancelled") return
            val request = requests[jobId]
            val publicMessage = if (request?.tool == CreationTool.IMAGE_CREATOR.wireName) {
                publicImageCreationFailure()
            } else {
                publicCreationText(message)
            }
            request?.outputPath?.let(::File)?.delete()
            jobs[jobId] = current.copy(
                stage = "failed",
                progressText = "Could not create result.",
                phase = "failed",
                error = publicMessage,
            )
            FailureRecord(
                request?.tool,
                publicMessage,
                request?.generationMode,
                request?.provider,
            )
        }
        diagnostics.event(
            "job_failed",
            failure.tool,
            jobId = jobId,
            stage = "failed",
            failureMessage = failure.message,
            generationMode = failure.generationMode,
            provider = failure.provider,
        )
    }

    private fun withElapsed(status: CreationJobStatus): CreationJobStatus {
        val id = status.jobId ?: return status
        val start = startedAt[id] ?: return status
        return if (creationStageIsBusy(status.stage)) {
            status.copy(elapsedMs = System.currentTimeMillis() - start)
        } else {
            status
        }
    }

    private fun runningCount(tool: CreationTool): Int = jobs.values.count {
        requestTool(it.jobId) == tool && creationStageIsBusy(it.stage)
    }

    private fun requestTool(jobId: String?): CreationTool? = jobId?.let(requests::get)
        ?.tool
        ?.let { CreationTool.fromWireName(it) }

    private fun nextJobId(tool: CreationTool): String =
        "${tool.wireName}_${System.currentTimeMillis()}_${sequence.getAndIncrement()}"

    private data class Continuation(
        val workerKey: String,
        val token: String,
        val sourcePath: String,
        val outputPath: String,
        val outputName: String,
        val provider: String,
    )

    private data class Completion(
        val request: CreationWorkerRequest,
        val segmented: Boolean,
        val publishedPath: String,
    )

    private data class FailureRecord(
        val tool: String?,
        val message: String,
        val generationMode: String?,
        val provider: String?,
    )

    companion object {
        private const val DISPATCH_WAIT_LOG_SECONDS = 60
        private val sequence = AtomicLong()
        @Volatile private var instance: CreationJobManager? = null

        fun get(context: Context): CreationJobManager = instance ?: synchronized(this) {
            instance ?: CreationJobManager(context.applicationContext).also { instance = it }
        }

    }
}

internal fun creationStageIsBusy(stage: String): Boolean = stage in setOf(
    "preparing",
    "uploading",
    "visualizing",
    "generating",
    "segmenting",
    "finalizing",
)

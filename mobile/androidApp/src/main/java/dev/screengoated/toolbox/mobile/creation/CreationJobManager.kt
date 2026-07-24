package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive
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
        val source = args.string("imagePath") ?: error("Pick an image first")
        require(files.exists(source)) { "Image does not exist" }
        synchronized(lock) {
            require(runningCount(tool) < CreationContract.MAXIMUM_PARALLEL_JOBS) {
                "Both creation workers are busy"
            }
        }
        val jobId = nextJobId(tool)
        val extension = if (tool == CreationTool.IMAGE_TO_3D) "glb" else "svg"
        val model = if (args.string("model") == "detail") "detail" else "simple"
        val polycount = (args.int("polycount") ?: CreationContract.DEFAULT_POLYCOUNT).coerceIn(
            CreationContract.MINIMUM_POLYCOUNT,
            CreationContract.MAXIMUM_POLYCOUNT,
        )
        val requestedAutoSegment = args.boolean("autoSegment") == true &&
            args.string("segmentationMode") != "none"
        val requestedMode = CreationGenerationMode.fromWireName(args.string("generationMode"))
        val providerRoute = if (tool == CreationTool.IMAGE_TO_3D) {
            CreationContract.validate3dProvider(
                requestedMode,
                polycount,
                requestedAutoSegment,
                args.string("provider"),
            )
        } else {
            null
        }
        val output = files.stagingFile(tool, source, extension)
        val request = CreationWorkerRequest(
            jobId = jobId,
            tool = tool.wireName,
            generationMode = providerRoute?.mode?.wireName,
            provider = providerRoute?.provider?.wireName,
            operation = "generate",
            imagePath = source,
            outputPath = output.absolutePath,
            outputName = output.name,
            polycount = providerRoute?.polycount ?: polycount,
            autoSegment = providerRoute?.autoSegment ?: requestedAutoSegment,
            model = model,
        )
        val status = initialStatus(tool, request)
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
            taskId = continuation.taskId,
            previousOutputPath = continuation.outputPath,
        )
        val status = initialStatus(CreationTool.IMAGE_TO_3D, request).copy(
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
            ?: idleStatus(tool)
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
                    val route = runCatching {
                        routeCreationWorkerFailure(expectedProvider, error)
                    }.getOrElse { invalidOwner ->
                        fail(jobId, invalidOwner.message ?: "Creation failed")
                        return@launch
                    }
                    when (route) {
                        CreationWorkerFailureRoute.Fail -> fail(jobId, error)
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
            val nextStage = event.stage ?: current.stage
            if (nextStage != current.stage || event.progressKey != null) {
                changedStage = event.progressKey ?: nextStage
            }
            jobs[jobId] = current.copy(
                stage = nextStage,
                progressText = event.progressText?.let(::publicCreationText)
                    ?: current.progressText,
                phase = event.phase ?: current.phase,
                workspaceState = event.workspaceState ?: current.workspaceState,
                progressRatio = event.progressRatio ?: current.progressRatio,
                estimatedTotalMs = event.estimatedTotalMs ?: current.estimatedTotalMs,
                timingSampleCount = event.timingSampleCount ?: current.timingSampleCount,
                generationMode = event.generationMode ?: current.generationMode,
                provider = event.provider ?: current.provider,
                outputPath = event.outputPath ?: current.outputPath,
                outputName = event.outputName ?: current.outputName,
                isSegmented = event.isSegmented ?: current.isSegmented,
                canSegment = event.canSegment ?: current.canSegment,
                faces = event.faces ?: current.faces,
                vertices = event.vertices ?: current.vertices,
            )
            requests[jobId]?.tool
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
                if (!creationStageIsBusy(current.stage)) return
                val staging = File(event.outputPath ?: request.outputPath)
                require(staging.isFile && staging.length() > 0L) {
                    "Creation ended without an output file"
                }
                val mime = if (request.tool == "3d") "model/gltf-binary" else "image/svg+xml"
                val published = files.publish(staging, request.outputName, mime)
                request.previousOutputPath?.takeIf(files::exists)?.let(files::delete)
                val segmented = request.provider == CreationProvider.MESHY.wireName ||
                    (event.isSegmented ?: request.autoSegment)
                val updated = current.copy(
                    stage = "done",
                    progressText = if (request.tool == "3d") "Model ready." else "Vector ready",
                    phase = "complete",
                    progressRatio = 1.0,
                    outputPath = published,
                    outputName = request.outputName,
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
                    creditsRemaining = event.creditsRemaining,
                    faces = event.faces,
                    vertices = event.vertices,
                    error = null,
                )
                jobs[jobId] = updated
                if (updated.canSegment && event.taskId != null) {
                    continuations[jobId] = Continuation(
                        workerKey,
                        event.taskId,
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
            } else {
                put("model", request.model)
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
        val publicMessage = publicCreationText(message)
        val tool = synchronized(lock) {
            val current = jobs[jobId] ?: return@synchronized null
            if (current.stage == "cancelled") return@synchronized null
            requests[jobId]?.outputPath?.let(::File)?.delete()
            jobs[jobId] = current.copy(
                stage = "failed",
                progressText = "Could not create result.",
                phase = "failed",
                error = publicMessage,
            )
            requests[jobId]?.tool
        }
        diagnostics.event(
            "job_failed",
            tool,
            jobId = jobId,
            stage = "failed",
            failureMessage = publicMessage,
            generationMode = synchronized(lock) { requests[jobId]?.generationMode },
            provider = synchronized(lock) { requests[jobId]?.provider },
        )
    }

    private fun initialStatus(tool: CreationTool, request: CreationWorkerRequest) = CreationJobStatus(
        jobId = request.jobId,
        generationMode = request.generationMode,
        provider = request.provider,
        polycount = request.polycount.takeIf { tool == CreationTool.IMAGE_TO_3D },
        autoSegment = request.autoSegment.takeIf { tool == CreationTool.IMAGE_TO_3D },
        stage = "preparing",
        progressText = "Preparing creation.",
        phase = "preparing",
        workspaceState = "checking",
        elapsedMs = 0,
        estimatedTotalMs = when {
            tool == CreationTool.IMAGE_TO_SVG && request.model == "detail" -> 70_000
            tool == CreationTool.IMAGE_TO_SVG -> 45_000
            request.provider == CreationProvider.MESHY.wireName -> 90_000
            request.autoSegment -> 360_000
            else -> 240_000
        },
        timingSampleCount = 0,
        progressRatio = 0.0,
        sourceImagePath = request.imagePath,
        model = request.model.takeIf { tool == CreationTool.IMAGE_TO_SVG },
    )

    private fun idleStatus(tool: CreationTool) = CreationJobStatus(
        generationMode = CreationGenerationMode.QUALITY.wireName.takeIf {
            tool == CreationTool.IMAGE_TO_3D
        },
        provider = CreationProvider.TRIPO.wireName.takeIf {
            tool == CreationTool.IMAGE_TO_3D
        },
        stage = if (tool == CreationTool.IMAGE_TO_3D) "idle" else "draft",
        progressText = "Ready to create.",
        sourceImagePath = if (tool == CreationTool.IMAGE_TO_SVG) "" else null,
        model = if (tool == CreationTool.IMAGE_TO_SVG) "simple" else null,
    )

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
        val taskId: String,
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
    "visualizing",
    "generating",
    "segmenting",
    "finalizing",
)

private fun JsonObject.string(key: String): String? = this[key]?.jsonPrimitive?.contentOrNull
private fun JsonObject.int(key: String): Int? = this[key]?.jsonPrimitive?.intOrNull
private fun JsonObject.boolean(key: String): Boolean? = this[key]?.jsonPrimitive?.booleanOrNull

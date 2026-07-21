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
        val output = files.stagingFile(tool, source, extension)
        val model = if (args.string("model") == "detail") "detail" else "simple"
        val autoSegment = args.boolean("autoSegment") == true &&
            args.string("segmentationMode") != "none"
        val request = CreationWorkerRequest(
            jobId = jobId,
            tool = tool.wireName,
            operation = "generate",
            imagePath = source,
            outputPath = output.absolutePath,
            outputName = output.name,
            polycount = (args.int("polycount") ?: CreationContract.DEFAULT_POLYCOUNT).coerceIn(
                CreationContract.MINIMUM_POLYCOUNT,
                CreationContract.MAXIMUM_POLYCOUNT,
            ),
            autoSegment = autoSegment,
            model = model,
        )
        val status = initialStatus(tool, request)
        synchronized(lock) {
            jobs[jobId] = status
            requests[jobId] = request
            startedAt[jobId] = System.currentTimeMillis()
        }
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
        val jobId = nextJobId(CreationTool.IMAGE_TO_3D)
        val output = files.stagingFile(CreationTool.IMAGE_TO_3D, continuation.sourcePath, "glb")
        val request = CreationWorkerRequest(
            jobId = jobId,
            tool = CreationTool.IMAGE_TO_3D.wireName,
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
                .filter { requestTool(it.jobId) == tool && isBusy(it.stage) }
                .mapNotNull { it.jobId }
            ids.forEach { id ->
                jobs[id]?.let { jobs[id] = it.copy(stage = "cancelled", progressText = "Cancelled.") }
                requests[id]?.outputPath?.let(::File).let { file -> if (file?.length() == 0L) file.delete() }
            }
            ids
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
            repeat(DISPATCH_WAIT_SECONDS) {
                if (synchronized(lock) { jobs[request.jobId]?.stage == "cancelled" }) return@launch
                val worker = workers.dispatch(request, preferred, ::handleWorkerEvent)
                if (worker != null) {
                    synchronized(lock) { workerKeys[request.jobId] = worker }
                    return@launch
                }
                delay(1_000)
            }
            fail(request.jobId, "No creation workspace became ready")
        }
    }

    private fun handleWorkerEvent(workerKey: String, event: CreationWorkerEvent) {
        val jobId = event.jobId ?: return
        scope.launch {
            if (event.event == "success") finish(workerKey, jobId, event)
            else if (event.event == "failure") fail(jobId, event.error ?: "Creation failed")
            else if (event.event == "cancelled") cancel(requestTool(jobId) ?: return@launch, jobId)
            else updateProgress(jobId, event)
        }
    }

    private fun updateProgress(jobId: String, event: CreationWorkerEvent) = synchronized(lock) {
        val current = jobs[jobId] ?: return@synchronized
        if (current.stage == "cancelled") return@synchronized
        jobs[jobId] = current.copy(
            stage = event.stage ?: current.stage,
            progressText = event.progressText ?: current.progressText,
            phase = event.phase ?: current.phase,
            progressRatio = event.progressRatio ?: current.progressRatio,
            estimatedTotalMs = event.estimatedTotalMs ?: current.estimatedTotalMs,
        )
    }

    private fun finish(workerKey: String, jobId: String, event: CreationWorkerEvent) {
        val request = synchronized(lock) { requests[jobId] } ?: return
        val staging = File(event.outputPath ?: request.outputPath)
        if (!staging.isFile || staging.length() == 0L) {
            fail(jobId, "Creation ended without an output file")
            return
        }
        runCatching {
            val mime = if (request.tool == "3d") "model/gltf-binary" else "image/svg+xml"
            val published = files.publish(staging, request.outputName, mime)
            request.previousOutputPath?.takeIf(files::exists)?.let(files::delete)
            val segmented = event.isSegmented ?: request.autoSegment
            val status = synchronized(lock) {
                val current = jobs[jobId] ?: error("Creation job disappeared")
                val updated = current.copy(
                    stage = "done",
                    progressText = if (request.tool == "3d") "Model ready." else "Vector ready",
                    phase = "complete",
                    progressRatio = 1.0,
                    outputPath = published,
                    outputName = request.outputName,
                    isSegmented = segmented,
                    canSegment = request.tool == "3d" && !segmented && event.canSegment != false,
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
                    )
                }
                updated
            }
            val tool = CreationTool.fromWireName(request.tool) ?: error("Unknown creation tool")
            val metadata = buildJsonObject {
                if (tool == CreationTool.IMAGE_TO_3D) {
                    put("isSegmented", segmented)
                    event.faces?.let { put("faces", it) }
                    event.vertices?.let { put("vertices", it) }
                } else {
                    put("model", request.model)
                }
            }
            history.record(tool, request.imagePath, published, request.outputName, metadata)
            status
        }.onFailure { fail(jobId, it.message ?: "Could not save creation result") }
    }

    private fun fail(jobId: String, message: String) = synchronized(lock) {
        val current = jobs[jobId] ?: return@synchronized
        if (current.stage == "cancelled") return@synchronized
        requests[jobId]?.outputPath?.let(::File)?.delete()
        jobs[jobId] = current.copy(
            stage = "failed",
            progressText = "Could not create result.",
            phase = "failed",
            error = message,
        )
    }

    private fun initialStatus(tool: CreationTool, request: CreationWorkerRequest) = CreationJobStatus(
        jobId = request.jobId,
        stage = "preparing",
        progressText = "Preparing creation.",
        phase = "preparing",
        workspaceState = "checking",
        elapsedMs = 0,
        estimatedTotalMs = when {
            tool == CreationTool.IMAGE_TO_SVG && request.model == "detail" -> 70_000
            tool == CreationTool.IMAGE_TO_SVG -> 45_000
            request.autoSegment -> 360_000
            else -> 240_000
        },
        progressRatio = 0.0,
        sourceImagePath = request.imagePath,
        model = request.model.takeIf { tool == CreationTool.IMAGE_TO_SVG },
    )

    private fun idleStatus(tool: CreationTool) = CreationJobStatus(
        stage = if (tool == CreationTool.IMAGE_TO_3D) "idle" else "draft",
        progressText = "Ready to create.",
        sourceImagePath = if (tool == CreationTool.IMAGE_TO_SVG) "" else null,
        model = if (tool == CreationTool.IMAGE_TO_SVG) "simple" else null,
    )

    private fun withElapsed(status: CreationJobStatus): CreationJobStatus {
        val id = status.jobId ?: return status
        val start = startedAt[id] ?: return status
        return if (isBusy(status.stage)) status.copy(elapsedMs = System.currentTimeMillis() - start) else status
    }

    private fun runningCount(tool: CreationTool): Int = jobs.values.count {
        requestTool(it.jobId) == tool && isBusy(it.stage)
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
    )

    companion object {
        private const val DISPATCH_WAIT_SECONDS = 10 * 60
        private val sequence = AtomicLong()
        @Volatile private var instance: CreationJobManager? = null

        fun get(context: Context): CreationJobManager = instance ?: synchronized(this) {
            instance ?: CreationJobManager(context.applicationContext).also { instance = it }
        }

        private fun isBusy(stage: String): Boolean = stage in setOf(
            "preparing", "visualizing", "generating", "segmenting", "finalizing",
        )
    }
}

private fun JsonObject.string(key: String): String? = this[key]?.jsonPrimitive?.contentOrNull
private fun JsonObject.int(key: String): Int? = this[key]?.jsonPrimitive?.intOrNull
private fun JsonObject.boolean(key: String): Boolean? = this[key]?.jsonPrimitive?.booleanOrNull

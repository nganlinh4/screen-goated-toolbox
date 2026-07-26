package dev.screengoated.toolbox.mobile.creation

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.Handler
import android.os.IBinder
import android.os.RemoteException
import android.util.Log
import dev.screengoated.toolbox.mobile.creation.worker.ICreationWorker
import dev.screengoated.toolbox.mobile.creation.worker.ICreationWorkerCallback
import dev.screengoated.toolbox.mobile.creation.worker.ImageCreatorWorker0Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageCreatorWorker1Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageCreatorWorker2Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageCreatorWorker3Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageTo3dWorker0Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageTo3dWorker1Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageTo3dWorker2Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageTo3dWorker3Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageToSvgWorker0Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageToSvgWorker1Service
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeManager
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeStatus
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json

internal class CreationWorkerPool private constructor(private val context: Context) {
    private val json = Json { ignoreUnknownKeys = true }
    private val diagnostics = CreationDiagnostics(context, "pool")
    private val runtime = CreationRuntimeManager.get(context)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val workers = listOf(
        Worker("3d-0", CreationTool.IMAGE_TO_3D, ImageTo3dWorker0Service::class.java, slot = 0),
        Worker("3d-1", CreationTool.IMAGE_TO_3D, ImageTo3dWorker1Service::class.java, slot = 1),
        Worker("3d-2", CreationTool.IMAGE_TO_3D, ImageTo3dWorker2Service::class.java, slot = 2),
        Worker("3d-3", CreationTool.IMAGE_TO_3D, ImageTo3dWorker3Service::class.java, slot = 3),
        Worker("svg-0", CreationTool.IMAGE_TO_SVG, ImageToSvgWorker0Service::class.java),
        Worker("svg-1", CreationTool.IMAGE_TO_SVG, ImageToSvgWorker1Service::class.java),
        Worker("image-0", CreationTool.IMAGE_CREATOR, ImageCreatorWorker0Service::class.java, slot = 0),
        Worker("image-1", CreationTool.IMAGE_CREATOR, ImageCreatorWorker1Service::class.java, slot = 1),
        Worker("image-2", CreationTool.IMAGE_CREATOR, ImageCreatorWorker2Service::class.java, slot = 2),
        Worker("image-3", CreationTool.IMAGE_CREATOR, ImageCreatorWorker3Service::class.java, slot = 3),
    )
    private val handler = Handler(context.mainLooper)
    private val jobWorkers = ConcurrentHashMap<String, String>()
    private val pendingBindings = mutableMapOf<String, Runnable>()
    @Volatile private var preferredPreparationTool: CreationTool? = null
    @Volatile private var nextPreparationStartAtMs = 0L
    @Volatile private var runtimeAwaiting = false

    init {
        check(workers.count { it.tool == CreationTool.IMAGE_TO_3D } == CreationContract.IMAGE_TO_3D_WORKSPACES)
        check(
            workers.filter { it.tool == CreationTool.IMAGE_TO_3D }.mapNotNull { it.slot }.toSet() ==
                (0 until CreationContract.IMAGE_TO_3D_WORKSPACES).toSet(),
        )
        check(workers.count { it.tool == CreationTool.IMAGE_TO_SVG } == CreationContract.IMAGE_TO_SVG_WORKSPACES)
        check(workers.count { it.tool == CreationTool.IMAGE_CREATOR } == CreationContract.IMAGE_CREATOR_WORKSPACES)
        check(
            workers.filter { it.tool == CreationTool.IMAGE_CREATOR }.mapNotNull { it.slot }.toSet() ==
                (0 until CreationContract.IMAGE_CREATOR_WORKSPACES).toSet(),
        )
    }

    fun startPreparation(priority: CreationTool? = null) {
        if (priority != null) {
            preferredPreparationTool = priority
            val unrelated = pendingBindings.keys.filter { key ->
                workers.firstOrNull { it.key == key }?.tool != priority
            }
            unrelated.forEach { key ->
                pendingBindings.remove(key)?.let(handler::removeCallbacks)
            }
        }
        if (runtime.factory() == null) {
            runtime.startInstall()
            awaitRuntime(priority)
            return
        }
        val ordered = if (priority == null) {
            CreationTool.entries.mapNotNull { tool -> workers.firstOrNull { it.tool == tool } }
        } else {
            workers.filter { it.tool == priority }
        }
        ordered.forEachIndexed { index, worker ->
            if (worker.binder != null || worker.binding || pendingBindings.containsKey(worker.key)) {
                return@forEachIndexed
            }
            lateinit var action: Runnable
            action = Runnable {
                pendingBindings.remove(worker.key, action)
                bind(worker)
            }
            pendingBindings[worker.key] = action
            handler.postDelayed(
                action,
                if (priority != null && index == 0) 0L else STARTUP_GRACE_MS + index * PREPARATION_STAGGER_MS,
            )
        }
    }

    fun preparationStatus(tool: CreationTool): String {
        when (runtime.status.value) {
            is CreationRuntimeStatus.Downloading -> return "preparing"
            is CreationRuntimeStatus.Missing,
            is CreationRuntimeStatus.Failed -> return "idle"
            is CreationRuntimeStatus.Ready -> Unit
        }
        val matching = workers.filter { it.tool == tool }
        val ready = matching.count { it.ready }
        val now = System.currentTimeMillis()
        return when {
            ready == matching.size -> "ready"
            ready > 0 -> "partial"
            matching.any {
                it.preparing || it.binding ||
                    (it.prepareScheduled && it.prepareNotBeforeMs <= now)
            } -> "preparing"
            else -> "idle"
        }
    }

    fun dispatch(
        request: CreationWorkerRequest,
        preferredWorker: String? = null,
        onEvent: (String, CreationWorkerEvent) -> Unit,
        onAssigned: (String) -> Unit = {},
    ): String? {
        val tool = CreationTool.fromWireName(request.tool) ?: return null
        val assignment = synchronized(workers) {
            val worker = if (preferredWorker != null) {
                workers.firstOrNull {
                    it.key == preferredWorker &&
                        it.binder != null &&
                        !it.busy &&
                        it.canRun(request)
                }
            } else {
                workers.firstOrNull {
                    it.tool == tool &&
                        it.ready &&
                        !it.busy &&
                        it.canRun(request)
                }
            } ?: return@synchronized null
            val binder = worker.binder ?: return@synchronized null
            worker.busy = true
            worker.ready = false
            worker.ownedJobReady = false
            worker.assignment.claim(request.jobId, onEvent)
            jobWorkers[request.jobId] = worker.key
            Assignment(worker, binder)
        } ?: return null
        val worker = assignment.worker
        val callback = callback(worker, request.jobId, onEvent)
        onAssigned(worker.key)
        return try {
            assignment.binder.runJob(
                json.encodeToString(CreationWorkerRequest.serializer(), request),
                callback,
            )
            worker.key
        } catch (_: RemoteException) {
            release(worker, request.jobId)
            synchronized(workers) { worker.binder = null }
            bind(worker)
            null
        }
    }

    fun cancel(jobId: String) {
        val key = jobWorkers[jobId] ?: return
        val worker = workers.firstOrNull { it.key == key } ?: return
        runCatching { worker.binder?.cancel(jobId) }
    }

    fun removeRuntime() {
        shutdown()
        runtime.delete()
    }

    private fun shutdown() {
        pendingBindings.values.forEach(handler::removeCallbacks)
        pendingBindings.clear()
        synchronized(workers) {
            workers.forEach { worker ->
                worker.assignment.jobId?.let { runCatching { worker.binder?.cancel(it) } }
                worker.connection?.let { connection ->
                    runCatching { context.unbindService(connection) }
                }
                context.stopService(Intent(context, worker.serviceClass))
                worker.binder = null
                worker.connection = null
                worker.binding = false
                worker.prepareScheduled = false
                worker.preparing = false
                worker.ready = false
                worker.ownedJobReady = false
                worker.busy = false
                worker.assignment.lose()
            }
            jobWorkers.clear()
        }
    }

    private fun awaitRuntime(priority: CreationTool?) {
        synchronized(this) {
            if (runtimeAwaiting) return
            runtimeAwaiting = true
        }
        scope.launch {
            val available = runtime.awaitFactory() != null
            runtimeAwaiting = false
            if (available) {
                handler.post { startPreparation(priority ?: preferredPreparationTool) }
            } else {
                diagnostics.event(
                    "runtime_unavailable",
                    priority?.wireName ?: "creation",
                    failureMessage = (runtime.status.value as? CreationRuntimeStatus.Failed)?.message,
                )
            }
        }
    }

    private fun bind(worker: Worker) {
        if (worker.binding || worker.binder != null) return
        Log.i(TAG, "Binding creation worker ${worker.key}")
        diagnostics.event("worker_binding", worker.tool.wireName, stage = worker.key)
        worker.binding = true
        val connection = object : ServiceConnection {
            private var connectedEpoch = -1L

            override fun onServiceConnected(name: ComponentName, service: IBinder) {
                synchronized(workers) {
                    worker.binding = false
                    worker.connectionEpoch += 1
                    connectedEpoch = worker.connectionEpoch
                    worker.binder = ICreationWorker.Stub.asInterface(service)
                }
                service.linkToDeath(
                    {
                        handleWorkerLoss(worker, connectedEpoch, "worker_died")
                    },
                    0,
                )
                requestPrepare(worker)
            }

            override fun onServiceDisconnected(name: ComponentName) {
                handleWorkerLoss(worker, connectedEpoch, "worker_disconnected")
            }
        }
        worker.connection = connection
        val bound = context.bindService(
            Intent(context, worker.serviceClass),
            connection,
            Context.BIND_AUTO_CREATE,
        )
        if (!bound) worker.binding = false
    }

    private fun requestPrepare(worker: Worker, delayMs: Long = 0L) {
        val schedule = synchronized(workers) {
            if (worker.prepareScheduled || worker.preparing || worker.busy || worker.ready ||
                worker.ownedJobReady || worker.binder == null
            ) {
                false
            } else {
                worker.prepareScheduled = true
                true
            }
        }
        if (!schedule) return
        handler.postDelayed(
            {
                synchronized(workers) { worker.prepareScheduled = false }
                prepare(worker)
            },
            delayMs,
        )
    }

    private fun prepare(worker: Worker) {
        val binder = worker.binder ?: return
        val waitMs = synchronized(workers) {
            val now = System.currentTimeMillis()
            val preparationNotBefore = maxOf(
                nextPreparationStartAtMs,
                worker.prepareNotBeforeMs,
            )
            when {
                worker.preparing || worker.busy || worker.ready || worker.ownedJobReady -> null
                workers.any { it.busy } -> PREPARATION_QUEUE_POLL_MS
                workers.any { it !== worker && it.preparing } -> PREPARATION_QUEUE_POLL_MS
                preparationNotBefore > now -> preparationNotBefore - now
                else -> {
                    worker.preparing = true
                    nextPreparationStartAtMs = now + MINIMUM_PREPARATION_INTERVAL_MS
                    -1L
                }
            }
        } ?: return
        if (waitMs >= 0L) {
            requestPrepare(worker, waitMs)
            return
        }
        try {
            binder.prepare(
                object : ICreationWorkerCallback.Stub() {
                    override fun onEvent(eventJson: String) {
                        val event = runCatching {
                            json.decodeFromString(CreationWorkerEvent.serializer(), eventJson)
                        }.getOrNull() ?: return
                        if (event.event == "ready") {
                            synchronized(workers) {
                                worker.ready = event.ready != false
                                worker.ownedJobReady = event.ownedJobReady == true
                                worker.availableModels = event.availableModels
                                worker.prepareNotBeforeMs = event.retryAfterMs ?: 0L
                                worker.preparing = false
                                worker.preparationFailures = 0
                            }
                            Log.i(TAG, "Creation worker ${worker.key} is ready")
                            diagnostics.event("worker_ready", worker.tool.wireName, stage = worker.key)
                            if (event.ready == false && event.retryAfterMs != null) {
                                requestPrepare(
                                    worker,
                                    (event.retryAfterMs - System.currentTimeMillis())
                                        .coerceAtLeast(PREPARATION_HANDOFF_GAP_MS),
                                )
                            }
                            requestNextPreparation(PREPARATION_HANDOFF_GAP_MS)
                        } else if (event.event == "failure") {
                            val error = event.error.orEmpty()
                            synchronized(workers) {
                                worker.ready = false
                                worker.ownedJobReady = false
                                worker.preparing = false
                                worker.preparationFailures += 1
                                worker.prepareNotBeforeMs = maxOf(
                                    worker.prepareNotBeforeMs,
                                    event.retryAfterMs ?: 0L,
                                )
                            }
                            val imageFailure = worker.tool == CreationTool.IMAGE_CREATOR
                            val category = if (imageFailure) {
                                IMAGE_CREATION_FAILURE_CATEGORY
                            } else {
                                CreationDiagnostics.failureCategory(error)
                            }
                            Log.w(TAG, "Creation worker ${worker.key} preparation failed: $category")
                            diagnostics.event(
                                "worker_prepare_failed",
                                worker.tool.wireName,
                                stage = worker.key,
                                failureMessage = error.takeUnless { imageFailure },
                                failureCategoryOverride = category.takeIf { imageFailure },
                            )
                            schedulePrepare(worker)
                            requestNextPreparation(PREPARATION_HANDOFF_GAP_MS)
                        }
                    }
                },
            )
        } catch (_: RemoteException) {
            synchronized(workers) {
                worker.preparing = false
                worker.binder = null
            }
            bind(worker)
        }
    }

    private fun requestNextPreparation(delayMs: Long) {
        val next = synchronized(workers) {
            workers.asSequence()
                .filter {
                    it.binder != null && !it.ready && !it.ownedJobReady &&
                        !it.busy && !it.preparing &&
                        !it.prepareScheduled
                }
                .sortedWith(
                    compareBy<Worker>(
                        { if (it.tool == preferredPreparationTool) 0 else 1 },
                        { it.preparationFailures },
                        { it.key },
                    ),
                )
                .firstOrNull()
        }
        next?.let { requestPrepare(it, delayMs) }
    }

    private fun callback(
        worker: Worker,
        jobId: String,
        onEvent: (String, CreationWorkerEvent) -> Unit,
    ): ICreationWorkerCallback = object : ICreationWorkerCallback.Stub() {
        override fun onEvent(eventJson: String) {
            val event = runCatching {
                json.decodeFromString(CreationWorkerEvent.serializer(), eventJson)
            }.getOrNull() ?: return
            val terminal = event.event == "success" ||
                event.event == "failure" ||
                event.event == "cancelled"
            val accepted = synchronized(workers) {
                if (!worker.assignment.owns(jobId, onEvent)) {
                    false
                } else {
                    if (terminal) {
                        worker.assignment.release(jobId)
                        jobWorkers.remove(jobId, worker.key)
                        worker.busy = false
                    }
                    true
                }
            }
            if (!accepted) return
            onEvent(worker.key, event)
            if (terminal) schedulePrepare(worker)
        }
    }

    private fun release(worker: Worker, jobId: String) {
        synchronized(workers) {
            if (worker.assignment.release(jobId) == null) return
            jobWorkers.remove(jobId, worker.key)
            worker.busy = false
        }
    }

    private fun handleWorkerLoss(worker: Worker, epoch: Long, diagnosticEvent: String) {
        val lostAssignment = synchronized(workers) {
            if (worker.connectionEpoch != epoch) return
            val assignment = worker.assignment.lose()
            assignment?.jobId?.let { jobWorkers.remove(it, worker.key) }
            worker.binder = null
            worker.ready = false
            worker.ownedJobReady = false
            worker.busy = false
            assignment
        }
        diagnostics.event(diagnosticEvent, worker.tool.wireName, stage = worker.key)
        lostAssignment?.sink?.invoke(
            worker.key,
            CreationWorkerEvent(
                jobId = lostAssignment.jobId,
                event = "failure",
                error = "Creation worker disconnected. Retry this creation.",
            ),
        )
        bind(worker)
    }

    private fun schedulePrepare(worker: Worker) {
        val delay = RETRY_DELAYS_MS[
            worker.preparationFailures.coerceIn(0, RETRY_DELAYS_MS.lastIndex)
        ]
        val retryAfterDelay =
            (worker.prepareNotBeforeMs - System.currentTimeMillis()).coerceAtLeast(0L)
        requestPrepare(worker, maxOf(delay, retryAfterDelay))
    }

    private data class Worker(
        val key: String,
        val tool: CreationTool,
        val serviceClass: Class<*>,
        val slot: Int? = null,
        @Volatile var binder: ICreationWorker? = null,
        @Volatile var connection: ServiceConnection? = null,
        @Volatile var binding: Boolean = false,
        @Volatile var prepareScheduled: Boolean = false,
        @Volatile var preparing: Boolean = false,
        @Volatile var ready: Boolean = false,
        @Volatile var ownedJobReady: Boolean = false,
        @Volatile var availableModels: List<String>? = null,
        @Volatile var prepareNotBeforeMs: Long = 0L,
        @Volatile var busy: Boolean = false,
        val assignment: CreationWorkerAssignmentGuard = CreationWorkerAssignmentGuard(),
        @Volatile var connectionEpoch: Long = 0,
        @Volatile var preparationFailures: Int = 0,
    ) {
        fun canRun(request: CreationWorkerRequest): Boolean {
            if (tool != CreationTool.fromWireName(request.tool)) return false
            if (tool == CreationTool.IMAGE_TO_SVG) {
                return availableModels?.contains(request.model) != false
            }
            return tool != CreationTool.IMAGE_TO_3D ||
                CreationContract.canUse3dWorker(request.provider, requireNotNull(slot))
        }
    }

    private data class Assignment(
        val worker: Worker,
        val binder: ICreationWorker,
    )

    companion object {
        private const val TAG = "CreationWorkerPool"
        private const val IMAGE_CREATION_FAILURE_CATEGORY = "image_creation"
        private const val STARTUP_GRACE_MS = 8_000L
        private const val PREPARATION_STAGGER_MS = 25_000L
        private const val PREPARATION_QUEUE_POLL_MS = 10_000L
        private const val PREPARATION_HANDOFF_GAP_MS = 8_000L
        private const val MINIMUM_PREPARATION_INTERVAL_MS =
            CreationContract.MINIMUM_PREPARATION_INTERVAL_SECONDS * 1_000L
        private val RETRY_DELAYS_MS = longArrayOf(15_000L, 30_000L, 60_000L, 120_000L, 300_000L)
        @Volatile private var instance: CreationWorkerPool? = null

        fun get(context: Context): CreationWorkerPool = instance ?: synchronized(this) {
            instance ?: CreationWorkerPool(context.applicationContext).also { instance = it }
        }
    }
}

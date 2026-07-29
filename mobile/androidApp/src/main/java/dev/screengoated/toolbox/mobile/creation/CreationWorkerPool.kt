package dev.screengoated.toolbox.mobile.creation

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.Handler
import android.os.IBinder
import android.os.RemoteException
import android.util.Log
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeManager
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeStatus
import dev.screengoated.toolbox.mobile.creation.runtime.runtimeSupportsOptionalInstruction
import dev.screengoated.toolbox.mobile.creation.worker.ICreationWorker
import dev.screengoated.toolbox.mobile.creation.worker.ICreationWorkerCallback
import dev.screengoated.toolbox.mobile.creation.worker.ImageCreatorWorker0Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageCreatorWorker1Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageTo3dWorker0Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageTo3dWorker1Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageToSvgWorker0Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageToSvgWorker1Service
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json

internal class CreationWorkerPool private constructor(private val context: Context) {
    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
        explicitNulls = false
    }
    private val runtime = CreationRuntimeManager.get(context)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val workers = listOf(
        Worker("3d-0", CreationTool.IMAGE_TO_3D, ImageTo3dWorker0Service::class.java),
        Worker("3d-1", CreationTool.IMAGE_TO_3D, ImageTo3dWorker1Service::class.java),
        Worker("svg-0", CreationTool.IMAGE_TO_SVG, ImageToSvgWorker0Service::class.java),
        Worker("svg-1", CreationTool.IMAGE_TO_SVG, ImageToSvgWorker1Service::class.java),
        Worker("image-0", CreationTool.IMAGE_CREATOR, ImageCreatorWorker0Service::class.java),
        Worker("image-1", CreationTool.IMAGE_CREATOR, ImageCreatorWorker1Service::class.java),
    )
    private val handler = Handler(context.mainLooper)
    private val jobWorkers = ConcurrentHashMap<String, String>()
    private val leases = CreationWorkerLeaseRegistry()
    private val leaseLock = Any()
    private val startupQueue = ArrayDeque(CreationTool.entries)
    private val surfacePriority = ArrayDeque<CreationTool>()
    private var startupActive: CreationTool? = null
    private var activePreparationTool: CreationTool? = null
    private var startupStarted = false
    @Volatile private var runtimeAwaiting = false

    fun acquire(tool: CreationTool, owner: String) {
        synchronized(leaseLock) {
            leases.acquire(tool, owner)
            if (owner != STARTUP_LEASE) {
                surfacePriority.remove(tool)
                surfacePriority.addFirst(tool)
            }
        }
        schedulePreparation()
    }

    fun release(tool: CreationTool, owner: String) {
        val last = synchronized(leaseLock) { leases.release(tool, owner) }
        if (last) {
            shutdownTool(tool)
            synchronized(leaseLock) {
                if (activePreparationTool == tool) activePreparationTool = null
                surfacePriority.remove(tool)
            }
            schedulePreparation()
        }
    }

    fun startOneShotPreparation() {
        val start = synchronized(leaseLock) {
            if (startupStarted) return
            startupStarted = true
            nextStartupToolLocked()
        }
        start?.let { acquire(it, STARTUP_LEASE) }
    }

    private fun schedulePreparation() {
        if (runtime.factory() == null) {
            runtime.startInstall()
            awaitRuntime()
            return
        }
        val ready = synchronized(workers) {
            CreationTool.entries.filterTo(mutableSetOf()) { tool ->
                workers.filter { it.tool == tool }.all { it.ready }
            }
        }
        val selected = synchronized(leaseLock) {
            selectCreationPreparationTool(
                activePreparationTool,
                leases.retainedTools(),
                ready,
                surfacePriority.toList(),
                startupActive,
            )?.also { activePreparationTool = it }
        } ?: return
        workers.filter { it.tool == selected }.forEach(::bind)
    }

    fun preparationStatus(tool: CreationTool): String {
        when (runtime.status.value) {
            is CreationRuntimeStatus.Downloading -> return "busy"
            is CreationRuntimeStatus.Missing,
            is CreationRuntimeStatus.Failed,
            -> return "unavailable"
            is CreationRuntimeStatus.Ready -> Unit
        }
        val matching = workers.filter { it.tool == tool }
        return synchronized(workers) {
            when {
                matching.any { it.ready && !it.busy } -> "ready"
                matching.any { it.busy || it.preparing || it.binding || it.prepareScheduled } ->
                    "busy"
                else -> "unavailable"
            }
        }
    }

    fun supportsOptionalInstruction(mode: String): Boolean = runtime.factory()
        ?.runtimeManifest()
        ?.let { runtimeSupportsOptionalInstruction(it, mode) }
        ?: false

    fun dispatch(
        request: CreationWorkerRequest,
        preferredWorker: String? = null,
        onEvent: (String, CreationWorkerEvent) -> Unit,
        onAssigned: (String) -> Unit = {},
    ): String? {
        val tool = CreationTool.fromWireName(request.tool) ?: return null
        val requestJson = json.encodeToString(CreationWorkerRequest.serializer(), request)
        val assignment = synchronized(workers) {
            val candidates = workers.filter {
                it.tool == tool && it.binder != null && it.ready && !it.busy
            }
            val worker = preferredWorker
                ?.let { preferred -> candidates.firstOrNull { it.key == preferred } }
                ?: candidates.firstOrNull().takeIf { preferredWorker == null }
                ?: return@synchronized null
            val binder = worker.binder ?: return@synchronized null
            worker.busy = true
            worker.ready = false
            worker.assignment.claim(request.jobId, onEvent)
            jobWorkers[request.jobId] = worker.key
            Assignment(worker, binder)
        } ?: return null
        val worker = assignment.worker
        val callback = callback(worker, request.jobId, onEvent)
        if (runCatching { onAssigned(worker.key) }.isFailure) {
            release(worker, request.jobId)
            requestPrepare(worker)
            return null
        }
        return try {
            assignment.binder.runJob(requestJson, callback)
            worker.key
        } catch (_: RemoteException) {
            release(worker, request.jobId)
            handleWorkerLoss(worker, worker.connectionEpoch)
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
        synchronized(leaseLock) { leases.clear() }
        shutdownWorkers(workers)
    }

    private fun shutdownTool(tool: CreationTool) {
        shutdownWorkers(workers.filter { it.tool == tool })
    }

    private fun shutdownWorkers(selected: List<Worker>) {
        val actions = synchronized(workers) {
            selected.map { worker ->
                val action = ShutdownAction(
                    worker = worker,
                    binder = worker.binder,
                    connection = worker.connection,
                    assignment = worker.assignment.lose(),
                )
                worker.binder = null
                worker.connection = null
                worker.binding = false
                worker.prepareScheduled = false
                worker.preparing = false
                worker.ready = false
                worker.busy = false
                worker.connectionEpoch += 1
                action
            }.also { actions ->
                actions.mapNotNull { it.assignment?.jobId }.forEach(jobWorkers::remove)
            }
        }
        actions.forEach { action ->
            action.assignment?.jobId?.let { runCatching { action.binder?.cancel(it) } }
            action.connection?.let { runCatching { context.unbindService(it) } }
            context.stopService(Intent(context, action.worker.serviceClass))
            action.assignment?.sink?.invoke(
                action.worker.key,
                CreationWorkerEvent(
                    jobId = action.assignment.jobId,
                    event = "execution_lost",
                    failureCode = "execution_lost",
                ),
            )
        }
    }

    private fun awaitRuntime() {
        synchronized(this) {
            if (runtimeAwaiting) return
            runtimeAwaiting = true
        }
        scope.launch {
            val available = runtime.awaitFactory() != null
            runtimeAwaiting = false
            if (available) handler.post(::schedulePreparation)
        }
    }

    private fun bind(worker: Worker) {
        val connection = synchronized(workers) {
            if (worker.binding || worker.binder != null || !toolIsRequested(worker.tool)) {
                return
            }
            worker.binding = true
            createConnection(worker).also { worker.connection = it }
        }
        val bound = runCatching {
            context.bindService(
                Intent(context, worker.serviceClass),
                connection,
                Context.BIND_AUTO_CREATE,
            )
        }.getOrDefault(false)
        if (!bound) {
            synchronized(workers) {
                if (worker.connection === connection) {
                    worker.binding = false
                    worker.connection = null
                }
            }
            handler.postDelayed({ bind(worker) }, PREPARATION_RETRY_DELAY_MS)
        }
    }

    private fun createConnection(worker: Worker) = object : ServiceConnection {
        private var connectedEpoch = -1L

        override fun onServiceConnected(name: ComponentName, service: IBinder) {
            synchronized(workers) {
                worker.binding = false
                worker.connectionEpoch += 1
                connectedEpoch = worker.connectionEpoch
                worker.binder = ICreationWorker.Stub.asInterface(service)
            }
            val linked = runCatching {
                service.linkToDeath({ handleWorkerLoss(worker, connectedEpoch) }, 0)
            }.isSuccess
            if (linked) {
                requestPrepare(worker)
            } else {
                handleWorkerLoss(worker, connectedEpoch)
            }
        }

        override fun onServiceDisconnected(name: ComponentName) {
            handleWorkerLoss(worker, connectedEpoch)
        }
    }

    private fun requestPrepare(worker: Worker, delayMs: Long = 0L) {
        val epoch = synchronized(workers) {
            if (worker.prepareScheduled || worker.preparing || worker.busy || worker.ready ||
                worker.binder == null
            ) {
                return
            }
            worker.prepareScheduled = true
            worker.connectionEpoch
        }
        handler.postDelayed(
            {
                val current = synchronized(workers) {
                    if (worker.connectionEpoch != epoch || !worker.prepareScheduled) {
                        false
                    } else {
                        worker.prepareScheduled = false
                        true
                    }
                }
                if (current) prepare(worker)
            },
            delayMs.coerceAtLeast(0L),
        )
    }

    private fun prepare(worker: Worker) {
        val call = synchronized(workers) {
            val binder = worker.binder ?: return
            if (worker.preparing || worker.busy || worker.ready) return
            worker.preparing = true
            PreparedCall(binder, worker.connectionEpoch)
        }
        try {
            call.binder.prepare(prepareCallback(worker, call))
        } catch (_: RemoteException) {
            handleWorkerLoss(worker, call.epoch)
        }
    }

    private fun prepareCallback(
        worker: Worker,
        call: PreparedCall,
    ): ICreationWorkerCallback = object : ICreationWorkerCallback.Stub() {
        override fun onEvent(eventJson: String) {
            val event = decodeCreationWorkerEvent(eventJson)
            val current = synchronized(workers) {
                worker.connectionEpoch == call.epoch && worker.binder === call.binder
            }
            if (!current) return
            if (creationPreparationEventIsReady(event)) {
                synchronized(workers) {
                    worker.ready = true
                    worker.preparing = false
                }
                completeToolPreparation(worker.tool)
                return
            }
            synchronized(workers) {
                worker.ready = false
                worker.preparing = false
            }
            if (event?.event == "failure") {
                    Log.w(
                        TAG,
                        "Creation engine preparation failed: " +
                            publicCreationFailureCategory(event.failureCode),
                    )
            }
            schedulePrepare(worker)
        }
    }

    private fun schedulePrepare(worker: Worker) =
        requestPrepare(worker, PREPARATION_RETRY_DELAY_MS)

    private fun completeToolPreparation(tool: CreationTool) {
        val ready = synchronized(workers) {
            workers.filter { it.tool == tool }.all { it.ready }
        }
        if (!ready) return
        val startupCompleted = synchronized(leaseLock) {
            if (activePreparationTool != tool) return
            activePreparationTool = null
            surfacePriority.remove(tool)
            if (startupActive == tool) {
                startupActive = null
                true
            } else {
                false
            }
        }
        if (startupCompleted) {
            release(tool, STARTUP_LEASE)
            val next = synchronized(leaseLock) { nextStartupToolLocked() }
            next?.let { acquire(it, STARTUP_LEASE) }
        }
        schedulePreparation()
    }

    private fun nextStartupToolLocked(): CreationTool? =
        startupQueue.removeFirstOrNull()?.also { startupActive = it }

    private fun callback(
        worker: Worker,
        jobId: String,
        onEvent: (String, CreationWorkerEvent) -> Unit,
    ): ICreationWorkerCallback = object : ICreationWorkerCallback.Stub() {
        override fun onEvent(eventJson: String) {
            val event = decodeCreationWorkerEvent(eventJson)
            if (event?.jobId != jobId) {
                if (!release(worker, jobId)) return
                onEvent(
                    worker.key,
                    CreationWorkerEvent(
                        jobId = jobId,
                        event = "failure",
                        failureCode = "unexpected",
                    ),
                )
                requestPrepare(worker)
                return
            }
            val terminal = event.event in TERMINAL_EVENTS
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
            if (terminal) requestPrepare(worker)
        }
    }

    private fun release(worker: Worker, jobId: String): Boolean =
        synchronized(workers) {
            if (worker.assignment.release(jobId) == null) return@synchronized false
            jobWorkers.remove(jobId, worker.key)
            worker.busy = false
            true
        }

    private fun handleWorkerLoss(worker: Worker, epoch: Long) {
        val loss = synchronized(workers) {
            if (worker.connectionEpoch != epoch) return
            val assignment = worker.assignment.lose()
            assignment?.jobId?.let { jobWorkers.remove(it, worker.key) }
            val connection = worker.connection
            worker.binder = null
            worker.connection = null
            worker.binding = false
            worker.prepareScheduled = false
            worker.preparing = false
            worker.ready = false
            worker.busy = false
            worker.connectionEpoch += 1
            WorkerLoss(connection, assignment)
        }
        loss.connection?.let { runCatching { context.unbindService(it) } }
        loss.assignment?.sink?.invoke(
            worker.key,
            CreationWorkerEvent(
                jobId = loss.assignment.jobId,
                event = "execution_lost",
                failureCode = "execution_lost",
            ),
        )
        schedulePreparation()
    }

    private fun toolIsRequested(tool: CreationTool): Boolean =
        synchronized(leaseLock) { leases.retained(tool) }

    private data class Worker(
        val key: String,
        val tool: CreationTool,
        val serviceClass: Class<*>,
        @Volatile var binder: ICreationWorker? = null,
        @Volatile var connection: ServiceConnection? = null,
        @Volatile var binding: Boolean = false,
        @Volatile var prepareScheduled: Boolean = false,
        @Volatile var preparing: Boolean = false,
        @Volatile var ready: Boolean = false,
        @Volatile var busy: Boolean = false,
        val assignment: CreationWorkerAssignmentGuard = CreationWorkerAssignmentGuard(),
        @Volatile var connectionEpoch: Long = 0,
    )

    private data class Assignment(val worker: Worker, val binder: ICreationWorker)
    private data class PreparedCall(val binder: ICreationWorker, val epoch: Long)
    private data class WorkerLoss(
        val connection: ServiceConnection?,
        val assignment: CreationWorkerAssignment?,
    )
    private data class ShutdownAction(
        val worker: Worker,
        val binder: ICreationWorker?,
        val connection: ServiceConnection?,
        val assignment: CreationWorkerAssignment?,
    )

    companion object {
        private const val TAG = "CreationWorkerPool"
        private const val STARTUP_LEASE = "startup"
        private const val PREPARATION_RETRY_DELAY_MS = 15_000L
        private val TERMINAL_EVENTS = setOf("success", "failure", "cancelled")
        @Volatile private var instance: CreationWorkerPool? = null

        fun get(context: Context): CreationWorkerPool = instance ?: synchronized(this) {
            instance ?: CreationWorkerPool(context.applicationContext).also { instance = it }
        }
    }
}

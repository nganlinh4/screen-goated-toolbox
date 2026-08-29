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
import dev.screengoated.toolbox.mobile.creation.worker.creationWorkerStructurallySupports
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
        Worker("3d-0", CreationTool.IMAGE_TO_3D, 0, ImageTo3dWorker0Service::class.java),
        Worker("3d-1", CreationTool.IMAGE_TO_3D, 1, ImageTo3dWorker1Service::class.java),
        Worker("svg-0", CreationTool.IMAGE_TO_SVG, 0, ImageToSvgWorker0Service::class.java),
        Worker("svg-1", CreationTool.IMAGE_TO_SVG, 1, ImageToSvgWorker1Service::class.java),
        Worker("image-0", CreationTool.IMAGE_CREATOR, 0, ImageCreatorWorker0Service::class.java),
        Worker("image-1", CreationTool.IMAGE_CREATOR, 1, ImageCreatorWorker1Service::class.java),
    )
    private val handler = Handler(context.mainLooper)
    private val jobWorkers = ConcurrentHashMap<String, String>()
    private val leases = CreationWorkerLeaseRegistry()
    private val preparationFailures = CreationPreparationFailureRegistry()
    private val leaseLock = Any()
    private val surfacePriority = ArrayDeque<CreationTool>()
    private var activePreparationTool: CreationTool? = null
    @Volatile private var runtimeAwaiting = false
    @Volatile private var preparationStateListener: (() -> Unit)? = null

    fun acquireSurface(tool: CreationTool, owner: String, requiredWorkerKey: String? = null) =
        acquire(tool, owner, CreationWorkerLeaseKind.SURFACE, requiredWorkerKey)

    fun acquireRecovery(tool: CreationTool, owner: String, requiredWorkerKey: String? = null) =
        acquire(tool, owner, CreationWorkerLeaseKind.JOB, requiredWorkerKey)

    fun requireRecoveryWorker(tool: CreationTool, owner: String, workerKey: String) {
        val demand = synchronized(leaseLock) {
            leases.requireWorker(tool, owner, workerKey)
            leases.preparationDemand(tool)
        }
        trimToolCapacity(tool, demand)
        schedulePreparation()
    }

    private fun acquire(
        tool: CreationTool,
        owner: String,
        kind: CreationWorkerLeaseKind,
        requiredWorkerKey: String? = null,
    ) {
        if (!creationToolReleased(tool)) return
        synchronized(leaseLock) {
            leases.acquire(tool, owner, kind, requiredWorkerKey)
            surfacePriority.remove(tool)
            surfacePriority.addFirst(tool)
        }
        schedulePreparation()
    }

    fun release(tool: CreationTool, owner: String) {
        val (last, demand) = synchronized(leaseLock) {
            leases.release(tool, owner) to leases.preparationDemand(tool)
        }
        if (last) {
            shutdownTool(tool)
            synchronized(leaseLock) {
                if (activePreparationTool == tool) activePreparationTool = null
                surfacePriority.remove(tool)
            }
            schedulePreparation()
        } else {
            trimToolCapacity(tool, demand)
            schedulePreparation()
        }
    }

    fun restartPreparation(tool: CreationTool) {
        val restarted = synchronized(leaseLock) { preparationFailures.restart(tool) }
        if (restarted) schedulePreparation()
    }

    fun setPreparationStateListener(listener: () -> Unit) {
        preparationStateListener = listener
    }

    private fun schedulePreparation() {
        if (runtime.factory() == null) {
            runtime.startInstall()
            awaitRuntime()
            return
        }
        val demands = synchronized(leaseLock) {
            CreationTool.entries.associateWith { tool ->
                leases.preparationDemand(tool)
            }
        }
        val ready = synchronized(workers) {
            CreationTool.entries.filterTo(mutableSetOf()) { tool ->
                val matching = workers.filter { it.tool == tool }
                val demand = demands.getValue(tool)
                creationPreparationCapacitySatisfied(
                    matching.map(Worker::preparationState),
                    demand.capacity,
                    creationRequiredSlotIndexes(
                        matching.map(Worker::key),
                        demand.requiredWorkerKeys,
                    ),
                )
            }
        }
        val (selected, displaced) = synchronized(leaseLock) {
            val previous = activePreparationTool
            val next = selectCreationPreparationTool(
                activePreparationTool,
                preparationFailures.available(leases.retainedTools()),
                ready,
                surfacePriority.toList(),
            )
            activePreparationTool = next
            next to previous?.takeIf { it != next }
        }
        selected ?: return
        displaced?.let { tool ->
            val interrupted = synchronized(workers) {
                workers.filter { it.tool == tool && !it.ready && !it.busy }
            }
            shutdownWorkers(interrupted)
        }
        val nextWorker = synchronized(workers) {
            val matching = workers.filter { it.tool == selected }
            val demand = demands.getValue(selected)
            val slot = nextCreationPreparationSlot(
                matching.map(Worker::preparationState),
                demand.capacity,
                CreationContract.maximumConcurrentPreparations(selected),
                creationRequiredSlotIndexes(
                    matching.map(Worker::key),
                    demand.requiredWorkerKeys,
                ),
            )
            slot?.let(matching::get)
        }
        nextWorker?.let(::bind)
    }

    fun preparationStatus(tool: CreationTool): String {
        if (!creationToolReleased(tool)) return "unavailable"
        when (runtime.status.value) {
            is CreationRuntimeStatus.Downloading -> return "busy"
            is CreationRuntimeStatus.Missing,
            is CreationRuntimeStatus.Failed,
            is CreationRuntimeStatus.RemovalPending,
            -> return "unavailable"
            is CreationRuntimeStatus.Ready -> Unit
        }
        if (synchronized(leaseLock) { preparationFailures.isFailed(tool) }) {
            return "unavailable"
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
    ): CreationWorkerDispatchResult {
        val tool = CreationTool.fromWireName(request.tool)
            ?: return CreationWorkerDispatchResult.PreparationFailed
        if (!creationToolReleased(tool)) {
            return CreationWorkerDispatchResult.PreparationFailed
        }
        if (synchronized(leaseLock) { preparationFailures.isFailed(tool) }) {
            return CreationWorkerDispatchResult.TemporaryCapacityPause
        }
        if (preferredWorker != null && workers.none { it.tool == tool && it.key == preferredWorker }) {
            return CreationWorkerDispatchResult.PreparationFailed
        }
        val available = synchronized(workers) {
            workers.filter {
                it.tool == tool && it.binder != null && it.ready && !it.busy
            }
        }
        val supported = available.filter { worker ->
            creationWorkerStructurallySupports(tool, worker.executionIndex, request)
        }
        val assignment = synchronized(workers) {
            val candidates = supported.filter {
                it.binder != null && it.ready && !it.busy
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
        } ?: return CreationWorkerDispatchResult.Waiting
        val worker = assignment.worker
        val callback = callback(worker, request.jobId, onEvent)
        val requestJson = json.encodeToString(CreationWorkerRequest.serializer(), request)
        if (runCatching { onAssigned(worker.key) }.isFailure) {
            release(worker, request.jobId)
            requestPrepare(worker)
            return CreationWorkerDispatchResult.Waiting
        }
        return try {
            assignment.binder.runJob(requestJson, callback)
            CreationWorkerDispatchResult.Assigned(worker.key)
        } catch (_: RemoteException) {
            release(worker, request.jobId)
            handleWorkerLoss(worker, worker.connectionEpoch)
            CreationWorkerDispatchResult.Waiting
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
        synchronized(leaseLock) {
            leases.clear()
            preparationFailures.clear()
        }
        shutdownWorkers(workers)
    }

    private fun shutdownTool(tool: CreationTool) {
        shutdownWorkers(workers.filter { it.tool == tool })
    }

    private fun trimToolCapacity(tool: CreationTool, demand: CreationPreparationDemand) {
        val selected = synchronized(workers) {
            val matching = workers.filter { it.tool == tool }
            creationPreparationRetirementSlots(
                matching.map(Worker::preparationState),
                demand.capacity,
                creationRequiredSlotIndexes(
                    matching.map(Worker::key),
                    demand.requiredWorkerKeys,
                ),
            ).map(matching::get)
        }
        shutdownWorkers(selected)
    }

    private fun shutdownWorkers(selected: List<Worker>) {
        shutdownCreationWorkers(context, workers, selected, jobWorkers)
    }
    private fun awaitRuntime() {
        synchronized(this) {
            if (runtimeAwaiting) return
            runtimeAwaiting = true
        }
        scope.launch {
            val available = runtime.awaitFactory() != null
            runtimeAwaiting = false
            handler.post {
                if (available) {
                    schedulePreparation()
                } else {
                    synchronized(leaseLock) {
                        preparationFailures.markFailed(leases.retainedTools())
                        activePreparationTool = null
                    }
                    preparationStateListener?.invoke()
                }
            }
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
        } else {
            schedulePreparation()
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
        if (!creationToolReleased(worker.tool)) return
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
            Log.i(TAG, "preparation_event_received valid=${event != null} current=$current " +
                "terminal=${event?.event == "ready" || event?.event == "failure"}")
            if (!current) return
            when (creationPreparationEventDisposition(event)) {
                CreationPreparationEventDisposition.IN_PROGRESS -> return
                CreationPreparationEventDisposition.READY -> {
                    synchronized(workers) {
                        worker.ready = true
                        worker.preparing = false
                    }
                    synchronized(leaseLock) { preparationFailures.restart(worker.tool) }
                    completeToolPreparation(worker.tool)
                    preparationStateListener?.invoke()
                    return
                }
                CreationPreparationEventDisposition.RETRY -> Unit
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
            val hasAlternative = synchronized(workers) {
                val matching = workers.filter { it.tool == worker.tool }
                hasIndependentPreparationLane(
                    matching.map {
                        CreationPreparationSlotState(
                            connected = it.binder != null,
                            binding = it.binding || it.preparing || it.prepareScheduled,
                            ready = it.ready,
                            busy = it.busy,
                        )
                    },
                    matching.indexOf(worker),
                )
            }
            if (!hasAlternative) {
                synchronized(leaseLock) {
                    preparationFailures.markFailed(worker.tool)
                    activePreparationTool = activeCreationPreparationAfterFailure(
                        activePreparationTool,
                        worker.tool,
                    )
                }
            }
            handleWorkerLoss(worker, call.epoch, reschedule = !hasAlternative)
            if (hasAlternative) {
                handler.postDelayed(::schedulePreparation, PREPARATION_RETRY_DELAY_MS)
            }
            preparationStateListener?.invoke()
        }
    }

    private fun completeToolPreparation(tool: CreationTool) {
        val demand = synchronized(leaseLock) { leases.preparationDemand(tool) }
        val ready = synchronized(workers) {
            val matching = workers.filter { it.tool == tool }
            creationPreparationCapacitySatisfied(
                matching.map(Worker::preparationState),
                demand.capacity,
                creationRequiredSlotIndexes(
                    matching.map(Worker::key),
                    demand.requiredWorkerKeys,
                ),
            )
        }
        if (!ready) {
            schedulePreparation()
            return
        }
        trimToolCapacity(tool, demand)
        synchronized(leaseLock) {
            if (activePreparationTool != tool) return
            activePreparationTool = null
            surfacePriority.remove(tool)
        }
        schedulePreparation()
    }

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
            val followUpReady = creationWorkerCanServeFollowUp(event)
            val accepted = synchronized(workers) {
                if (!worker.assignment.owns(jobId, onEvent)) {
                    false
                } else {
                    if (terminal) {
                        worker.assignment.release(jobId)
                        jobWorkers.remove(jobId, worker.key)
                        worker.busy = false
                        worker.ready = followUpReady
                    }
                    true
                }
            }
            if (!accepted) return
            onEvent(worker.key, event)
            if (terminal && !followUpReady) requestPrepare(worker)
        }
    }

    private fun release(worker: Worker, jobId: String): Boolean =
        synchronized(workers) {
            if (worker.assignment.release(jobId) == null) return@synchronized false
            jobWorkers.remove(jobId, worker.key)
            worker.busy = false
            true
        }

    private fun handleWorkerLoss(worker: Worker, epoch: Long, reschedule: Boolean = true) {
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
        if (reschedule) schedulePreparation()
    }

    private fun toolIsRequested(tool: CreationTool): Boolean =
        creationToolReleased(tool) && synchronized(leaseLock) { leases.retained(tool) }

    companion object {
        private const val TAG = "CreationWorkerPool"
        private const val PREPARATION_RETRY_DELAY_MS = 15_000L
        private val TERMINAL_EVENTS = setOf("success", "failure", "cancelled")
        @Volatile private var instance: CreationWorkerPool? = null

        fun get(context: Context): CreationWorkerPool = instance ?: synchronized(this) {
            instance ?: CreationWorkerPool(context.applicationContext).also { instance = it }
        }
    }
}

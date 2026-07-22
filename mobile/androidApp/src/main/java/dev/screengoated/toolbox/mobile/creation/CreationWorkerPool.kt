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
import dev.screengoated.toolbox.mobile.creation.worker.ImageTo3dWorker0Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageTo3dWorker1Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageTo3dWorker2Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageTo3dWorker3Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageToSvgWorker0Service
import dev.screengoated.toolbox.mobile.creation.worker.ImageToSvgWorker1Service
import java.util.concurrent.ConcurrentHashMap
import kotlinx.serialization.json.Json

internal class CreationWorkerPool private constructor(private val context: Context) {
    private val json = Json { ignoreUnknownKeys = true }
    private val diagnostics = CreationDiagnostics(context, "pool")
    private val preparationPreferences = context.getSharedPreferences(
        PREPARATION_PREFERENCES,
        Context.MODE_PRIVATE,
    )
    private val workers = listOf(
        Worker("3d-0", CreationTool.IMAGE_TO_3D, ImageTo3dWorker0Service::class.java),
        Worker("3d-1", CreationTool.IMAGE_TO_3D, ImageTo3dWorker1Service::class.java),
        Worker("3d-2", CreationTool.IMAGE_TO_3D, ImageTo3dWorker2Service::class.java),
        Worker("3d-3", CreationTool.IMAGE_TO_3D, ImageTo3dWorker3Service::class.java),
        Worker("svg-0", CreationTool.IMAGE_TO_SVG, ImageToSvgWorker0Service::class.java),
        Worker("svg-1", CreationTool.IMAGE_TO_SVG, ImageToSvgWorker1Service::class.java),
    )
    private val handler = Handler(context.mainLooper)
    private val jobWorkers = ConcurrentHashMap<String, String>()
    private val pendingBindings = mutableMapOf<String, Runnable>()
    @Volatile private var preferredPreparationTool: CreationTool? = null
    @Volatile private var preparationCooldownUntilMs = maxOf(
        preparationPreferences.getLong(PREPARATION_COOLDOWN_KEY, 0L),
        CreationPreparationCooldown.read(context),
    ).also { CreationPreparationCooldown.recordUntil(context, it) }
    @Volatile private var nextPreparationStartAtMs = 0L

    init {
        check(workers.count { it.tool == CreationTool.IMAGE_TO_3D } == CreationContract.IMAGE_TO_3D_WORKSPACES)
        check(workers.count { it.tool == CreationTool.IMAGE_TO_SVG } == CreationContract.IMAGE_TO_SVG_WORKSPACES)
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
        val matching = workers.filter { it.tool == tool }
        val ready = matching.count { it.ready }
        return when {
            ready == matching.size -> "ready"
            ready > 0 -> "partial"
            matching.any { it.preparing || it.binding || it.prepareScheduled } -> "preparing"
            else -> "idle"
        }
    }

    fun dispatch(
        request: CreationWorkerRequest,
        preferredWorker: String? = null,
        onEvent: (String, CreationWorkerEvent) -> Unit,
    ): String? {
        val tool = CreationTool.fromWireName(request.tool) ?: return null
        val worker = synchronized(workers) {
            preferredWorker?.let { key ->
                workers.firstOrNull { it.key == key && it.binder != null && !it.busy }
            }
                ?: workers.firstOrNull { it.tool == tool && it.ready && !it.busy }
        } ?: return null
        val binder = worker.binder ?: return null
        worker.busy = true
        worker.ready = false
        worker.activeJobId = request.jobId
        jobWorkers[request.jobId] = worker.key
        val callback = callback(worker, request.jobId, onEvent)
        return try {
            binder.runJob(json.encodeToString(CreationWorkerRequest.serializer(), request), callback)
            worker.key
        } catch (_: RemoteException) {
            release(worker, request.jobId)
            bind(worker)
            null
        }
    }

    fun cancel(jobId: String) {
        val key = jobWorkers[jobId] ?: return
        val worker = workers.firstOrNull { it.key == key } ?: return
        runCatching { worker.binder?.cancel(jobId) }
    }

    private fun bind(worker: Worker) {
        if (worker.binding || worker.binder != null) return
        Log.i(TAG, "Binding creation worker ${worker.key}")
        diagnostics.event("worker_binding", worker.tool.wireName, stage = worker.key)
        worker.binding = true
        val connection = object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName, service: IBinder) {
                worker.binding = false
                worker.binder = ICreationWorker.Stub.asInterface(service)
                service.linkToDeath(
                    {
                        worker.binder = null
                        worker.ready = false
                        worker.busy = false
                        worker.activeJobId?.let(jobWorkers::remove)
                        worker.activeJobId = null
                        diagnostics.event("worker_died", worker.tool.wireName, stage = worker.key)
                        bind(worker)
                    },
                    0,
                )
                requestPrepare(worker)
            }

            override fun onServiceDisconnected(name: ComponentName) {
                worker.binder = null
                worker.ready = false
                worker.busy = false
                worker.activeJobId?.let(jobWorkers::remove)
                worker.activeJobId = null
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
                worker.binder == null
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
            preparationCooldownUntilMs = maxOf(
                preparationCooldownUntilMs,
                CreationPreparationCooldown.read(context),
            )
            val preparationNotBefore = maxOf(
                if (worker.mailboxBlocked) preparationCooldownUntilMs else 0L,
                nextPreparationStartAtMs,
            )
            when {
                worker.preparing || worker.busy || worker.ready -> null
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
                                worker.preparing = false
                                worker.preparationFailures = 0
                                worker.mailboxBlocked = false
                                CreationPreparationCooldown.recordPreparationSucceeded(context)
                            }
                            Log.i(TAG, "Creation worker ${worker.key} is ready")
                            diagnostics.event("worker_ready", worker.tool.wireName, stage = worker.key)
                            requestNextPreparation(PREPARATION_HANDOFF_GAP_MS)
                        } else if (event.event == "failure") {
                            val error = event.error.orEmpty()
                            synchronized(workers) {
                                worker.ready = false
                                worker.preparing = false
                                worker.preparationFailures += 1
                                val mailboxFailure = isMailboxFailure(error)
                                worker.mailboxBlocked = mailboxFailure
                                if (mailboxFailure && !isExistingMailboxCooldown(error)) {
                                    recordMailboxFailureCooldown()
                                }
                            }
                            val category = CreationDiagnostics.failureCategory(error)
                            Log.w(TAG, "Creation worker ${worker.key} preparation failed: $category")
                            diagnostics.event(
                                "worker_prepare_failed",
                                worker.tool.wireName,
                                stage = worker.key,
                                failureMessage = error,
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
                    it.binder != null && !it.ready && !it.busy && !it.preparing &&
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
            onEvent(worker.key, event)
            if (event.event == "success" || event.event == "failure" || event.event == "cancelled") {
                release(worker, jobId)
                schedulePrepare(worker)
            }
        }
    }

    private fun release(worker: Worker, jobId: String) {
        jobWorkers.remove(jobId)
        worker.busy = false
        worker.activeJobId = null
    }

    private fun schedulePrepare(worker: Worker) {
        val delay = RETRY_DELAYS_MS[
            worker.preparationFailures.coerceIn(0, RETRY_DELAYS_MS.lastIndex)
        ]
        preparationCooldownUntilMs = maxOf(
            preparationCooldownUntilMs,
            CreationPreparationCooldown.read(context),
        )
        val cooldown = if (worker.mailboxBlocked) {
            (preparationCooldownUntilMs - System.currentTimeMillis()).coerceAtLeast(0L)
        } else {
            0L
        }
        requestPrepare(worker, maxOf(delay, cooldown))
    }

    private fun recordMailboxFailureCooldown() {
        preparationCooldownUntilMs = CreationPreparationCooldown.recordMailboxFailure(context)
        preparationPreferences.edit()
            .putLong(PREPARATION_COOLDOWN_KEY, preparationCooldownUntilMs)
            .apply()
    }

    private fun isMailboxFailure(message: String): Boolean {
        return message.contains("mailbox", ignoreCase = true) ||
            message.contains("waiting for verification code", ignoreCase = true) ||
            message.contains("waiting for account confirmation", ignoreCase = true)
    }

    private fun isExistingMailboxCooldown(message: String): Boolean =
        message.contains("mailbox preparation is cooling down", ignoreCase = true)

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
        @Volatile var activeJobId: String? = null,
        @Volatile var preparationFailures: Int = 0,
        @Volatile var mailboxBlocked: Boolean = false,
    )

    companion object {
        private const val TAG = "CreationWorkerPool"
        private const val PREPARATION_PREFERENCES = "creation_worker_pool"
        private const val PREPARATION_COOLDOWN_KEY = "mailbox_cooldown_until_ms"
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

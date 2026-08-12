package dev.screengoated.toolbox.mobile.service.nativelibs

import android.content.Context
import android.util.Log
import com.google.android.play.core.splitcompat.SplitCompat
import com.google.android.play.core.splitinstall.SplitInstallManager
import com.google.android.play.core.splitinstall.SplitInstallManagerFactory
import com.google.android.play.core.splitinstall.SplitInstallRequest
import com.google.android.play.core.splitinstall.SplitInstallSessionState
import com.google.android.play.core.splitinstall.SplitInstallStateUpdatedListener
import com.google.android.play.core.splitinstall.model.SplitInstallSessionStatus
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.io.File
import java.util.concurrent.ConcurrentHashMap

/** Native ASR engines delivered exclusively by Google Play as on-demand modules. */
class NativeLibManager private constructor(context: Context) {
    private val context = context.applicationContext
    enum class Engine(
        val moduleName: String,
        val libs: List<String>,
    ) {
        ORT(
            moduleName = "feature_asr_ort",
            // The feature owns the complete payload. Loading uses the real runtime
            // directly; the API-table proxy remains for compatibility.
            libs = listOf(
                "libonnxruntime_real.so",
                "libonnxruntime.so",
            ),
        ),
        MOONSHINE(
            moduleName = "feature_asr_moonshine",
            libs = listOf("libmoonshine-jni.so", "libmoonshine.so"),
        ),
        SHERPA(
            moduleName = "feature_asr_sherpa",
            libs = listOf("libsherpa-onnx-jni.so"),
        ),
    }

    sealed class Status {
        data object Missing : Status()
        data class Downloading(val progress: Float) : Status()
        data class Installed(val sizeBytes: Long) : Status()
        data class RemovalPending(
            val message: String,
            val retryable: Boolean = false,
        ) : Status()
        data class Error(val message: String) : Status()
    }

    private val splitManager: SplitInstallManager =
        SplitInstallManagerFactory.create(context.applicationContext)
    private val removalStore = NativeRuntimeRemovalStore(this.context)
    private val leases = RuntimeLeaseRegistry<Engine>(::finishRemoval)
    private val statuses = Engine.entries.associateWith { MutableStateFlow(computeStatus(it)) }
    private val sessions = ConcurrentHashMap<Engine, Int>()
    private val installLeases = ConcurrentHashMap<Engine, AutoCloseable>()
    private val confirmationErrors = ConcurrentHashMap<Int, String>()
    private val uninstallRequests = ConcurrentHashMap.newKeySet<Engine>()
    private val listener = SplitInstallStateUpdatedListener(::onInstallState)

    init {
        splitManager.registerListener(listener)
        Engine.entries.filter { removalStore.isPending(it.name) }.forEach(leases::requestRemoval)
    }

    fun status(engine: Engine): StateFlow<Status> = requireNotNull(statuses[engine])

    fun isInstalled(engine: Engine): Boolean =
        contractMatches(engine) &&
            requiredModulesForPlay(engine).all { it in splitManager.installedModules }

    fun startDownload(engine: Engine) {
        val flow = requireNotNull(statuses[engine])
        if (flow.value is Status.Downloading || isInstalled(engine) || leases.isRemovalPending(engine)) return
        val installLease = leases.acquire(listOf(engine)) ?: return
        installLeases[engine] = installLease
        flow.value = Status.Downloading(0f)
        val requestBuilder = SplitInstallRequest.newBuilder()
        requiredModulesForPlay(engine).forEach(requestBuilder::addModule)
        val request = requestBuilder.build()
        splitManager.startInstall(request)
            .addOnSuccessListener { sessionId ->
                sessions[engine] = sessionId
                if (leases.isRemovalPending(engine)) splitManager.cancelInstall(sessionId)
            }
            .addOnFailureListener { error ->
                val removalRequested = removalStore.isPending(engine.name)
                installLeases.remove(engine)?.close()
                flow.value = if (removalRequested) {
                    computeStatus(engine)
                } else {
                    Status.Error(error.message ?: "Play feature install failed")
                }
            }
    }

    fun cancelDownload(engine: Engine) {
        sessions.remove(engine)?.let { sessionId ->
            confirmationErrors.remove(sessionId)
            PlaySplitInstallConfirmationCoordinator.release(sessionId)
            splitManager.cancelInstall(sessionId)
        }
        statuses[engine]?.value = computeStatus(engine)
    }

    fun cancelAllDownloads() = Engine.entries.forEach(::cancelDownload)

    fun delete(engine: Engine) {
        cancelDownload(engine)
        removalStore.setPending(engine.name, true)
        statuses[engine]?.value = Status.RemovalPending(removalMessage(engine))
        leases.requestRemoval(engine)
    }

    fun acquireLease(vararg engines: Engine): AutoCloseable? {
        val requested = engines.distinct()
        if (requested.any { !isInstalled(it) }) return null
        return leases.acquire(requested)
    }

    fun loadEngines(vararg engines: Engine): Boolean {
        if (engines.any { !isInstalled(it) || leases.isRemovalPending(it) }) {
            Log.w(TAG, "Native feature is not installed for requested engines")
            return false
        }
        if (!SplitCompat.install(context)) {
            Log.e(TAG, "SplitCompat could not activate installed native features")
            return false
        }
        val needed = engines.flatMap { it.libs }
        markLoaded(*engines)
        for (lib in NativeLibraryLoadContract.orderedDependencies(needed)) {
            try {
                System.loadLibrary(lib.removePrefix("lib").removeSuffix(".so"))
            } catch (error: UnsatisfiedLinkError) {
                if (error.message?.contains("already loaded") != true) {
                    Log.e(TAG, "Failed to load native dependency $lib", error)
                    return false
                }
            }
        }
        return true
    }

    private fun finishRemoval(engine: Engine) {
        statuses[engine]?.value = Status.RemovalPending(removalMessage(engine))
        if (!isInstalled(engine)) {
            completeRemoval(engine)
            return
        }
        if (!uninstallRequests.add(engine)) return
        // Play only guarantees that deferred uninstall is scheduled. Keep the
        // payload pending until a later process observes that its modules are gone.
        splitManager.deferredUninstall(requiredModulesForPlay(engine))
            .addOnSuccessListener {
                uninstallRequests -= engine
                if (isInstalled(engine)) {
                    statuses[engine]?.value = Status.RemovalPending(removalMessage(engine))
                } else {
                    completeRemoval(engine)
                }
            }
            .addOnFailureListener { error ->
                uninstallRequests -= engine
                statuses[engine]?.value = Status.RemovalPending(
                    error.message ?: "Play feature removal failed. Try again.",
                    retryable = true,
                )
            }
    }

    private fun completeRemoval(engine: Engine) {
        removalStore.setPending(engine.name, false)
        leases.completeRemoval(engine)
        statuses[engine]?.value = Status.Missing
    }

    private fun removalMessage(engine: Engine): String = when {
        leases.isInUse(engine) -> "Removal pending until the active session stops."
        isLoaded(engine) -> "Google Play scheduled removal. Restart the app after it completes."
        else -> "Google Play scheduled this runtime for removal."
    }

    private fun onInstallState(state: SplitInstallSessionState) {
        val engine = resolveEngine(state) ?: return
        sessions.putIfAbsent(engine, state.sessionId())
        val flow = requireNotNull(statuses[engine])
        when (state.status()) {
            SplitInstallSessionStatus.DOWNLOADING,
            SplitInstallSessionStatus.PENDING,
            SplitInstallSessionStatus.INSTALLING -> {
                PlaySplitInstallConfirmationCoordinator.promptNoLongerRequired(state.sessionId())
                val total = state.totalBytesToDownload()
                val progress = if (total > 0) state.bytesDownloaded().toFloat() / total else 0f
                flow.value = Status.Downloading(progress)
            }
            SplitInstallSessionStatus.REQUIRES_USER_CONFIRMATION -> {
                val total = state.totalBytesToDownload()
                val progress = if (total > 0) state.bytesDownloaded().toFloat() / total else 0f
                flow.value = Status.Downloading(progress)
                PlaySplitInstallConfirmationCoordinator.request(
                    context = context,
                    sessionId = state.sessionId(),
                    owner = this,
                    onFailure = { message -> failConfirmation(engine, state.sessionId(), message) },
                )
            }
            SplitInstallSessionStatus.INSTALLED -> {
                SplitCompat.install(context)
                releaseSession(engine, state.sessionId())
                flow.value = computeStatus(engine)
            }
            SplitInstallSessionStatus.CANCELED -> {
                val confirmationError = confirmationErrors.remove(state.sessionId())
                releaseSession(engine, state.sessionId())
                flow.value = confirmationError?.let(Status::Error) ?: computeStatus(engine)
            }
            SplitInstallSessionStatus.FAILED -> {
                val confirmationError = confirmationErrors.remove(state.sessionId())
                val removalRequested = removalStore.isPending(engine.name)
                releaseSession(engine, state.sessionId())
                flow.value = if (removalRequested) {
                    computeStatus(engine)
                } else {
                    Status.Error(
                        confirmationError ?: "Play feature install failed (${state.errorCode()})",
                    )
                }
            }
        }
    }

    private fun resolveEngine(state: SplitInstallSessionState): Engine? {
        sessions.entries.firstOrNull { it.value == state.sessionId() }?.let { return it.key }
        val modules = state.moduleNames().toSet()
        return Engine.entries.singleOrNull { requiredModulesForPlay(it).toSet() == modules }
    }

    private fun failConfirmation(engine: Engine, sessionId: Int, message: String) {
        confirmationErrors[sessionId] = message
        statuses[engine]?.value = Status.Error(message)
        splitManager.cancelInstall(sessionId).addOnFailureListener {
            releaseSession(engine, sessionId)
        }
    }

    private fun releaseSession(engine: Engine, sessionId: Int) {
        sessions.remove(engine, sessionId)
        installLeases.remove(engine)?.close()
        confirmationErrors.remove(sessionId)
        PlaySplitInstallConfirmationCoordinator.release(sessionId)
    }

    private fun computeStatus(engine: Engine): Status {
        val installed = isInstalled(engine)
        if (!installed && removalStore.isPending(engine.name)) {
            removalStore.setPending(engine.name, false)
        }
        return when (deferredRemovalState(installed, removalStore.isPending(engine.name))) {
            DeferredRemovalState.REMOVAL_PENDING ->
                Status.RemovalPending(removalMessage(engine))
            DeferredRemovalState.INSTALLED -> Status.Installed(installedSize(engine))
            DeferredRemovalState.MISSING -> Status.Missing
        }
    }

    private fun contractMatches(engine: Engine): Boolean = runCatching {
        val archive = NativeRuntimeContract.load(context).archive(engine.name.lowercase())
        val contractEntries = archive.entries.map { it.fileName }.toSet()
        contractEntries == engine.libs.toSet()
    }.getOrDefault(false)

    private fun installedSize(engine: Engine): Long {
        val splitPaths = context.applicationInfo.splitSourceDirs.orEmpty()
        return splitPaths
            .filter { path -> requiredModulesForPlay(engine).any(path::contains) }
            .sumOf { File(it).length() }
    }

    companion object {
        private const val TAG = "NativeLibManager"
        @Volatile private var instance: NativeLibManager? = null
        private val loadedEngines = mutableSetOf<Engine>()
        @Volatile private var moonshineLoaded = false
        @Volatile private var sherpaLoaded = false

        fun get(context: Context): NativeLibManager = instance ?: synchronized(this) {
            instance ?: NativeLibManager(context.applicationContext).also { instance = it }
        }

        fun reconcilePendingRemovals(context: Context) {
            val store = NativeRuntimeRemovalStore(context)
            if (Engine.entries.any { store.isPending(it.name) }) get(context)
        }

        @Synchronized
        private fun markLoaded(vararg engines: Engine) {
            loadedEngines.addAll(engines)
        }

        @Synchronized
        private fun isLoaded(engine: Engine): Boolean = engine in loadedEngines

        fun ensureMoonshineLoaded(context: Context): Boolean {
            if (moonshineLoaded) return true
            moonshineLoaded = get(context).loadEngines(Engine.ORT, Engine.MOONSHINE)
            return moonshineLoaded
        }

        fun ensureSherpaLoaded(context: Context): Boolean {
            if (sherpaLoaded) return true
            sherpaLoaded = get(context).loadEngines(Engine.SHERPA)
            return sherpaLoaded
        }

    }
}

internal fun requiredModulesForPlay(engine: NativeLibManager.Engine): List<String> =
    listOf(engine.moduleName)

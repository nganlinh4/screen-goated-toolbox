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
class NativeLibManager(private val context: Context) {
    enum class Engine(
        val moduleName: String,
        val libs: List<String>,
    ) {
        ORT(
            moduleName = "feature_asr_ort",
            // The feature owns the complete payload. Loading uses the real runtime
            // directly; the API-table proxy remains for compatibility.
            libs = listOf(
                "libc++_shared.so",
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
        data class Error(val message: String) : Status()
    }

    private val splitManager: SplitInstallManager =
        SplitInstallManagerFactory.create(context.applicationContext)
    private val statuses = Engine.entries.associateWith { MutableStateFlow(computeStatus(it)) }
    private val sessions = ConcurrentHashMap<Engine, Int>()
    private val confirmationErrors = ConcurrentHashMap<Int, String>()
    private val listener = SplitInstallStateUpdatedListener(::onInstallState)

    init {
        splitManager.registerListener(listener)
    }

    fun status(engine: Engine): StateFlow<Status> = requireNotNull(statuses[engine])

    fun isInstalled(engine: Engine): Boolean =
        contractMatches(engine) &&
            requiredModulesForPlay(engine).all { it in splitManager.installedModules }

    fun startDownload(engine: Engine) {
        val flow = requireNotNull(statuses[engine])
        if (flow.value is Status.Downloading || isInstalled(engine)) return
        flow.value = Status.Downloading(0f)
        val requestBuilder = SplitInstallRequest.newBuilder()
        requiredModulesForPlay(engine).forEach(requestBuilder::addModule)
        val request = requestBuilder.build()
        splitManager.startInstall(request)
            .addOnSuccessListener { sessionId -> sessions[engine] = sessionId }
            .addOnFailureListener { error ->
                flow.value = Status.Error(error.message ?: "Play feature install failed")
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
        // ORT is the only remaining consumer of the shared C++ runtime, so both
        // modules leave through the same tested delivery contract.
        val modules = requiredModulesForPlay(engine)
        splitManager.deferredUninstall(modules)
            .addOnSuccessListener { statuses[engine]?.value = Status.Missing }
            .addOnFailureListener { error ->
                statuses[engine]?.value =
                    Status.Error(error.message ?: "Play feature removal failed")
            }
    }

    fun loadEngines(vararg engines: Engine): Boolean {
        if (engines.any { !isInstalled(it) }) {
            Log.w(TAG, "Native feature is not installed for requested engines")
            return false
        }
        if (!SplitCompat.install(context)) {
            Log.e(TAG, "SplitCompat could not activate installed native features")
            return false
        }
        val needed = engines.flatMap { it.libs }
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
                releaseSession(engine, state.sessionId())
                flow.value = Status.Error(
                    confirmationError ?: "Play feature install failed (${state.errorCode()})",
                )
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
        confirmationErrors.remove(sessionId)
        PlaySplitInstallConfirmationCoordinator.release(sessionId)
    }

    private fun computeStatus(engine: Engine): Status =
        if (isInstalled(engine)) Status.Installed(installedSize(engine)) else Status.Missing

    private fun contractMatches(engine: Engine): Boolean = runCatching {
        val archive = NativeRuntimeContract.load(context).archive(engine.name.lowercase())
        archive.entries.map { it.fileName }.toSet() == engine.libs.toSet()
    }.getOrDefault(false)

    private fun installedSize(engine: Engine): Long {
        val splitPaths = context.applicationInfo.splitSourceDirs.orEmpty()
        return splitPaths
            .filter { path -> requiredModulesForPlay(engine).any(path::contains) }
            .sumOf { File(it).length() }
    }

    companion object {
        private const val TAG = "NativeLibManager"
        @Volatile private var moonshineLoaded = false
        @Volatile private var sherpaLoaded = false
        @Volatile private var ortLoaded = false

        @Synchronized
        fun ensureOrtLoaded(context: Context): Boolean {
            if (ortLoaded) return true
            if (!NativeLibManager(context).loadEngines(Engine.ORT)) return false
            ortLoaded = loadJavaOrtBridge()
            return ortLoaded
        }

        fun ensureMoonshineLoaded(context: Context): Boolean {
            if (moonshineLoaded) return true
            moonshineLoaded = NativeLibManager(context).loadEngines(Engine.ORT, Engine.MOONSHINE)
            return moonshineLoaded
        }

        fun ensureSherpaLoaded(context: Context): Boolean {
            if (sherpaLoaded) return true
            sherpaLoaded = NativeLibManager(context).loadEngines(Engine.SHERPA)
            return sherpaLoaded
        }

        private fun loadJavaOrtBridge(): Boolean = try {
            System.loadLibrary("onnxruntime4j_jni")
            true
        } catch (error: UnsatisfiedLinkError) {
            if (error.message?.contains("already loaded") == true) {
                true
            } else {
                Log.e(TAG, "Failed to load ONNX Java bridge", error)
                false
            }
        }
    }
}

internal fun requiredModulesForPlay(engine: NativeLibManager.Engine): List<String> =
    if (engine == NativeLibManager.Engine.ORT) {
        listOf(engine.moduleName, PLAY_NATIVE_CPP_MODULE)
    } else {
        listOf(engine.moduleName)
    }

private const val PLAY_NATIVE_CPP_MODULE = "feature_native_cpp"

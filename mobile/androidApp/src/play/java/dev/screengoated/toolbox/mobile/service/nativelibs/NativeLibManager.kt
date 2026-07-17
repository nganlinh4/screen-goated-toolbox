package dev.screengoated.toolbox.mobile.service.nativelibs

import android.content.Context
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

/** Native ASR engines delivered exclusively by Google Play as on-demand modules. */
class NativeLibManager(private val context: Context) {
    enum class Engine(
        val moduleName: String,
        val libs: List<String>,
    ) {
        ORT(
            moduleName = "feature_asr_ort",
            libs = listOf("libc++_shared.so", "libonnxruntime.so"),
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
    private val sessions = mutableMapOf<Engine, Int>()
    private val listener = SplitInstallStateUpdatedListener(::onInstallState)

    init {
        splitManager.registerListener(listener)
    }

    fun status(engine: Engine): StateFlow<Status> = requireNotNull(statuses[engine])

    fun isInstalled(engine: Engine): Boolean =
        requiredModules(engine).all { it in splitManager.installedModules }

    fun startDownload(engine: Engine) {
        val flow = requireNotNull(statuses[engine])
        if (flow.value is Status.Downloading || isInstalled(engine)) return
        flow.value = Status.Downloading(0f)
        val requestBuilder = SplitInstallRequest.newBuilder()
        requiredModules(engine).forEach(requestBuilder::addModule)
        val request = requestBuilder.build()
        splitManager.startInstall(request)
            .addOnSuccessListener { sessionId -> sessions[engine] = sessionId }
            .addOnFailureListener { error ->
                flow.value = Status.Error(error.message ?: "Play feature install failed")
            }
    }

    fun cancelDownload(engine: Engine) {
        sessions.remove(engine)?.let(splitManager::cancelInstall)
        statuses[engine]?.value = computeStatus(engine)
    }

    fun cancelAllDownloads() = Engine.entries.forEach(::cancelDownload)

    fun delete(engine: Engine) {
        val modules = buildList {
            add(engine.moduleName)
            // ORT is the only remaining consumer of the shared C++ runtime, so it leaves with it.
            if (engine == Engine.ORT) {
                add(NATIVE_CPP_MODULE)
            }
        }
        splitManager.deferredUninstall(modules)
            .addOnSuccessListener { statuses[engine]?.value = Status.Missing }
            .addOnFailureListener { error ->
                statuses[engine]?.value =
                    Status.Error(error.message ?: "Play feature removal failed")
            }
    }

    fun loadEngines(vararg engines: Engine): Boolean {
        if (engines.any { !isInstalled(it) }) return false
        if (!SplitCompat.install(context)) return false
        val needed = engines.flatMap { it.libs }.toSet()
        for (lib in LOAD_ORDER) {
            if (lib !in needed) continue
            try {
                System.loadLibrary(lib.removePrefix("lib").removeSuffix(".so"))
            } catch (error: UnsatisfiedLinkError) {
                if (error.message?.contains("already loaded") != true) return false
            }
        }
        return true
    }

    private fun onInstallState(state: SplitInstallSessionState) {
        val engine = sessions.entries.firstOrNull { it.value == state.sessionId() }?.key ?: return
        val flow = requireNotNull(statuses[engine])
        when (state.status()) {
            SplitInstallSessionStatus.DOWNLOADING,
            SplitInstallSessionStatus.REQUIRES_USER_CONFIRMATION,
            SplitInstallSessionStatus.PENDING,
            SplitInstallSessionStatus.INSTALLING -> {
                val total = state.totalBytesToDownload()
                val progress = if (total > 0) state.bytesDownloaded().toFloat() / total else 0f
                flow.value = Status.Downloading(progress)
            }
            SplitInstallSessionStatus.INSTALLED -> {
                SplitCompat.install(context)
                sessions.remove(engine)
                flow.value = computeStatus(engine)
            }
            SplitInstallSessionStatus.CANCELED -> {
                sessions.remove(engine)
                flow.value = computeStatus(engine)
            }
            SplitInstallSessionStatus.FAILED -> {
                sessions.remove(engine)
                flow.value = Status.Error("Play feature install failed (${state.errorCode()})")
            }
        }
    }

    private fun computeStatus(engine: Engine): Status =
        if (isInstalled(engine)) Status.Installed(installedSize(engine)) else Status.Missing

    private fun installedSize(engine: Engine): Long {
        val splitPaths = context.applicationInfo.splitSourceDirs.orEmpty()
        return splitPaths
            .filter { path -> requiredModules(engine).any(path::contains) }
            .sumOf { File(it).length() }
    }

    private fun requiredModules(engine: Engine): List<String> =
        if (engine == Engine.ORT) listOf(engine.moduleName, NATIVE_CPP_MODULE) else listOf(engine.moduleName)

    companion object {
        private val LOAD_ORDER = listOf(
            "libc++_shared.so",
            "libonnxruntime.so",
            "libmoonshine.so",
            "libmoonshine-jni.so",
            "libsherpa-onnx-jni.so",
        )
        private const val NATIVE_CPP_MODULE = "feature_native_cpp"

        @Volatile private var moonshineLoaded = false
        @Volatile private var sherpaLoaded = false

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
    }
}

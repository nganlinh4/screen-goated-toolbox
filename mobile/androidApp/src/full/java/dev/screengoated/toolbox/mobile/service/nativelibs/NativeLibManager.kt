package dev.screengoated.toolbox.mobile.service.nativelibs

import android.content.Context
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File

/**
 * Per-engine native library download and loading.
 *
 * Full-delivery native libraries. ORT is an immutable bundled asset; the other
 * runtimes are downloaded only when their exact checked-in archive identity is met.
 */
class NativeLibManager(private val context: Context) {

    enum class Engine(
        val zipName: String,
        val libs: List<String>,
    ) {
        /** ONNX Runtime — needed by Moonshine. */
        ORT(
            zipName = "ort-runtime.zip",
            // Readiness and cleanup cover the complete runtime payload. Loading uses
            // the real runtime directly; the API-table proxy remains for compatibility.
            libs = listOf(
                "libc++_shared.so",
                "libonnxruntime_real.so",
                "libonnxruntime.so",
            ),
        ),
        /** Moonshine Voice — English streaming ASR. */
        MOONSHINE(
            zipName = "moonshine-runtime.zip",
            libs = listOf("libmoonshine-jni.so", "libmoonshine.so"),
        ),
        /** Sherpa-ONNX — Zipformer multilingual ASR. */
        SHERPA(
            zipName = "sherpa-runtime.zip",
            libs = listOf("libsherpa-onnx-jni.so"),
        ),
    }

    sealed class Status {
        data object Missing : Status()
        data class Downloading(val progress: Float) : Status()
        data class Installed(val sizeBytes: Long) : Status()
        data class Error(val message: String) : Status()
    }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val httpClient = OkHttpClient()

    private val libDir: File
        get() = File(context.filesDir, "native-libs").also { it.mkdirs() }

    private val _statuses = mutableMapOf<Engine, MutableStateFlow<Status>>()
    private val downloadJobs = mutableMapOf<Engine, Job>()

    init {
        for (engine in Engine.entries) {
            _statuses[engine] = MutableStateFlow(computeStatus(engine))
        }
    }

    fun status(engine: Engine): StateFlow<Status> =
        _statuses.getOrPut(engine) { MutableStateFlow(computeStatus(engine)) }

    fun isInstalled(engine: Engine): Boolean = runCatching {
        VerifiedNativeArchive.isInstalled(libDir, archiveContract(engine))
    }.getOrDefault(false)

    fun startDownload(engine: Engine) {
        val flow = _statuses[engine] ?: return
        if (flow.value is Status.Downloading) return
        flow.value = Status.Downloading(0f)
        downloadJobs[engine]?.cancel()
        downloadJobs[engine] = scope.launch {
            try {
                downloadAndExtract(engine, flow)
                // Set read+execute (required for dlopen), no write (API 34+ DCL policy)
                for (lib in engine.libs) {
                    val f = File(libDir, lib)
                    f.setReadable(true)
                    f.setExecutable(true)
                    f.setWritable(false)
                }
                flow.value = computeStatus(engine)
            } catch (_: CancellationException) {
                delete(engine)
            } catch (e: Exception) {
                flow.value = Status.Error(e.message ?: "Download failed")
            } finally {
                downloadJobs.remove(engine)
            }
        }
    }

    fun cancelDownload(engine: Engine) {
        downloadJobs.remove(engine)?.cancel()
        _statuses[engine]?.value = computeStatus(engine)
    }

    fun cancelAllDownloads() {
        Engine.entries.forEach(::cancelDownload)
    }

    fun delete(engine: Engine) {
        for (lib in engine.libs) {
            val f = File(libDir, lib)
            if (f.exists()) f.delete()
        }
        _statuses[engine]?.value = Status.Missing
    }

    /**
     * Prepare all runtime libs for the given engines.
     *
     * Inject the download dir into the classloader namespace and load the
     * requested JNI libraries in dependency order via System.loadLibrary(name).
     */
    fun loadEngines(vararg engines: Engine): Boolean {
        for (engine in engines) {
            if (!isInstalled(engine)) return false
        }
        // Inject our download dir into the classloader's native lib search path.
        // This makes System.loadLibrary() and dlopen DT_NEEDED resolution find
        // our downloaded .so files by name (not just by absolute path).
        injectNativeLibDir()

        val needed = engines.flatMap { it.libs }
        val ordered = NativeLibraryLoadContract.orderedDependencies(needed)
        android.util.Log.i("NativeLibManager", "loadEngines: needed=$needed, dir=${libDir.absolutePath}")
        for (lib in ordered) {
            val f = File(libDir, lib)
            if (!f.exists()) {
                android.util.Log.w("NativeLibManager", "File missing: $lib")
                return false
            }
            try {
                val name = lib.removePrefix("lib").removeSuffix(".so")
                android.util.Log.i("NativeLibManager", "Loading: $name (via loadLibrary)")
                System.loadLibrary(name)
                android.util.Log.i("NativeLibManager", "OK: $name")
            } catch (e: UnsatisfiedLinkError) {
                if (e.message?.contains("already loaded") == true) {
                    android.util.Log.i("NativeLibManager", "Already loaded: $lib")
                } else {
                    android.util.Log.e("NativeLibManager", "Failed to load $lib", e)
                    return false
                }
            }
        }
        return true
    }

    private fun computeStatus(engine: Engine): Status {
        return if (isInstalled(engine)) {
            val bytes = archiveContract(engine).entries.sumOf { it.byteCount }
            Status.Installed(bytes)
        } else {
            Status.Missing
        }
    }

    private fun downloadAndExtract(engine: Engine, flow: MutableStateFlow<Status>) {
        val contract = archiveContract(engine)
        val zipFile = File(context.cacheDir, contract.fileName)
        try {
            if (contract.fullDelivery == "bundled_asset") {
                require(engine == Engine.ORT) { "Only ORT has a bundled Full runtime" }
                context.assets.open(NativeRuntimeContract.FULL_ORT_ASSET_PATH).use { source ->
                    VerifiedNativeArchive.materialize(source, zipFile, contract) { progress ->
                        flow.value = Status.Downloading(progress * 0.9f)
                    }
                }
            } else {
                require(contract.fullDelivery == "verified_download") {
                    "Unsupported Full native delivery: ${contract.fullDelivery}"
                }
                val request = Request.Builder().url("$BASE_URL/${contract.fileName}").build()
                httpClient.newCall(request).execute().use { response ->
                    if (!response.isSuccessful) throw Exception("HTTP ${response.code}")
                    val contentLength = response.body.contentLength()
                    require(contentLength < 0L || contentLength == contract.byteCount) {
                        "${contract.fileName} HTTP byte count differs: $contentLength"
                    }
                    VerifiedNativeArchive.materialize(
                        response.body.byteStream(),
                        zipFile,
                        contract,
                    ) { progress -> flow.value = Status.Downloading(progress * 0.9f) }
                }
            }
            flow.value = Status.Downloading(0.95f)
            VerifiedNativeArchive.install(zipFile, libDir, contract)
            flow.value = Status.Downloading(1.0f)
        } finally {
            zipFile.delete()
        }
    }

    private fun archiveContract(engine: Engine): NativeRuntimeArchive {
        val archive = NativeRuntimeContract.load(context).archive(engine.name.lowercase())
        require(archive.fileName == engine.zipName) {
            "Native runtime archive differs for ${engine.name}"
        }
        require(archive.entries.map { it.fileName }.toSet() == engine.libs.toSet()) {
            "Native runtime members differ for ${engine.name}"
        }
        return archive
    }

    // dirInjected lives in the companion so it persists across instances

    /**
     * Add our download dir to BaseDexClassLoader's native library search path
     * via reflection. This is the same technique Chrome and ReLinker use.
     * After injection, System.loadLibrary("foo") and dlopen DT_NEEDED
     * resolution will find libfoo.so in our download dir.
     */
    private fun injectNativeLibDir() {
        if (dirInjected) return
        try {
            val classLoader = context.classLoader
            // BaseDexClassLoader → pathList (DexPathList)
            val pathListField = classLoader.javaClass.superclass
                ?.getDeclaredField("pathList")
                ?: return
            pathListField.isAccessible = true
            val pathList = pathListField.get(classLoader) ?: return

            // DexPathList → nativeLibraryDirectories (List<File>)
            val nativeDirsField = pathList.javaClass.getDeclaredField("nativeLibraryDirectories")
            nativeDirsField.isAccessible = true
            @Suppress("UNCHECKED_CAST")
            val dirs = nativeDirsField.get(pathList) as? MutableList<File> ?: return

            val dir = libDir
            if (dirs.contains(dir)) {
                dirInjected = true
                return
            }

            // Add our dir to the front of the list
            val newDirs = ArrayList<File>()
            newDirs.add(dir)
            newDirs.addAll(dirs)
            nativeDirsField.set(pathList, newDirs)

            // Also rebuild nativeLibraryPathElements which is what's actually searched
            try {
                val makeElements = pathList.javaClass.getDeclaredMethod(
                    "makePathElements",
                    MutableList::class.java,
                )
                makeElements.isAccessible = true
                val elements = makeElements.invoke(null, newDirs)
                val elementsField = pathList.javaClass.getDeclaredField("nativeLibraryPathElements")
                elementsField.isAccessible = true
                elementsField.set(pathList, elements)
            } catch (_: NoSuchMethodException) {
                // Older Android — nativeLibraryDirectories alone may suffice
            }

            dirInjected = true
            android.util.Log.i("NativeLibManager", "Injected ${dir.absolutePath} into native lib path")
        } catch (e: Exception) {
            android.util.Log.w("NativeLibManager", "Failed to inject native lib path", e)
        }
    }

    companion object {
        @Volatile
        private var dirInjected = false

        @Volatile
        private var moonshineLoaded = false

        @Volatile
        private var sherpaLoaded = false

        @Volatile
        private var ortLoaded = false

        private const val BASE_URL =
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/mobile/androidApp/libs"

        @Synchronized
        fun ensureOrtLoaded(context: Context): Boolean {
            if (ortLoaded) return true
            if (!NativeLibManager(context).loadEngines(Engine.ORT)) return false
            ortLoaded = loadJavaOrtBridge()
            return ortLoaded
        }

        /** Ensure Moonshine libs are loaded. */
        fun ensureMoonshineLoaded(context: Context): Boolean {
            if (moonshineLoaded) return true
            val mgr = NativeLibManager(context)
            if (mgr.loadEngines(Engine.ORT, Engine.MOONSHINE)) {
                moonshineLoaded = true
                return true
            }
            return false
        }

        /** Ensure Sherpa libs are loaded (self-contained, ORT statically linked). */
        fun ensureSherpaLoaded(context: Context): Boolean {
            if (sherpaLoaded) return true
            val mgr = NativeLibManager(context)
            if (mgr.loadEngines(Engine.SHERPA)) {
                sherpaLoaded = true
                return true
            }
            return false
        }

        private fun loadJavaOrtBridge(): Boolean = try {
            System.loadLibrary("onnxruntime4j_jni")
            true
        } catch (error: UnsatisfiedLinkError) {
            if (error.message?.contains("already loaded") == true) {
                true
            } else {
                android.util.Log.e("NativeLibManager", "Failed to load ONNX Java bridge", error)
                false
            }
        }
    }
}

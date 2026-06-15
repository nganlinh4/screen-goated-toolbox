package dev.screengoated.toolbox.mobile.service.moonshine

import android.content.Context
import android.util.Log
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.util.concurrent.TimeUnit

/**
 * Manages on-demand download of Moonshine Voice models per language.
 *
 * Each language has its own model directory under files/models/moonshine/<model-name>/.
 * Streaming models (English) have different files than non-streaming (Base) models.
 */
class MoonshineModelManager(private val context: Context) {

    private val modelsRoot = File(context.filesDir, "models/moonshine")

    private val _downloadState = MutableStateFlow<DownloadState>(DownloadState.Idle)
    val downloadState: StateFlow<DownloadState> = _downloadState.asStateFlow()

    private val _zipformerStatuses = MutableStateFlow(
        ZipformerLanguage.entries.associateWith { lang ->
            if (isZipformerInstalled(lang)) {
                val size = zipformerDir(lang).walkTopDown().sumOf { if (it.isFile) it.length() else 0L }
                ZipformerLangStatus.Installed(size)
            } else {
                ZipformerLangStatus.Missing
            }
        }
    )
    val zipformerStatuses: StateFlow<Map<ZipformerLanguage, ZipformerLangStatus>> = _zipformerStatuses.asStateFlow()

    private val _moonshineStatuses = MutableStateFlow(
        MoonshineLanguage.entries.associateWith { lang ->
            if (isInstalled(lang)) {
                val size = modelDir(lang).walkTopDown().sumOf { if (it.isFile) it.length() else 0L }
                MoonshineLangStatus.Installed(size)
            } else {
                MoonshineLangStatus.Missing
            }
        }
    )
    val moonshineStatuses: StateFlow<Map<MoonshineLanguage, MoonshineLangStatus>> = _moonshineStatuses.asStateFlow()

    // Outlives any individual UI scope — downloads continue after the dialog is dismissed
    private val managerScope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    private val client = OkHttpClient.Builder()
        .connectTimeout(30, TimeUnit.SECONDS)
        .readTimeout(120, TimeUnit.SECONDS)
        .build()

    /** Check if a language model is downloaded and ready. */
    fun isInstalled(lang: MoonshineLanguage): Boolean {
        val dir = modelDir(lang)
        return dir.exists() && lang.modelFiles.all { File(dir, it).exists() }
    }

    /** Get the filesystem path for a Moonshine model directory. */
    fun modelDir(lang: MoonshineLanguage): File = File(modelsRoot, lang.modelName)

    /** Get the filesystem path for a Zipformer model directory. */
    fun zipformerDir(lang: ZipformerLanguage): File = File(modelsRoot, lang.modelName)

    fun isZipformerInstalled(lang: ZipformerLanguage): Boolean {
        val dir = zipformerDir(lang)
        return dir.exists() && lang.modelFiles.all { File(dir, it).exists() }
    }

    suspend fun downloadZipformer(lang: ZipformerLanguage) {
        if (isZipformerInstalled(lang)) return
        withContext(Dispatchers.IO) {
            val dir = zipformerDir(lang)
            dir.mkdirs()

            val files = lang.modelFiles
            val fileWeight = 1f / files.size
            for ((idx, filename) in files.withIndex()) {
                val target = File(dir, filename)
                if (target.exists()) continue
                val baseProgress = idx.toFloat() / files.size
                val downloadingStatus = ZipformerLangStatus.Downloading(baseProgress)
                _zipformerStatuses.value = _zipformerStatuses.value.toMutableMap().also {
                    it[lang] = downloadingStatus
                }
                _downloadState.value = DownloadState.Downloading(
                    progress = baseProgress,
                    currentFile = filename,
                    language = lang.displayName,
                )
                val url = "${lang.downloadBaseUrl}/$filename"
                try {
                    downloadZipformerFile(lang, url, target, baseProgress, fileWeight)
                } catch (e: CancellationException) {
                    target.delete()
                    File(dir, "$filename.part").delete()
                    _zipformerStatuses.value = _zipformerStatuses.value.toMutableMap().also {
                        it[lang] = ZipformerLangStatus.Missing
                    }
                    _downloadState.value = DownloadState.Idle
                    throw e
                } catch (e: Exception) {
                    target.delete()
                    File(dir, "$filename.part").delete()
                    val errorMsg = "Failed to download $filename: ${e.message}"
                    _zipformerStatuses.value = _zipformerStatuses.value.toMutableMap().also {
                        it[lang] = ZipformerLangStatus.Error(errorMsg)
                    }
                    _downloadState.value = DownloadState.Error(errorMsg)
                    return@withContext
                }
            }
            val size = dir.walkTopDown().sumOf { if (it.isFile) it.length() else 0L }
            _zipformerStatuses.value = _zipformerStatuses.value.toMutableMap().also {
                it[lang] = ZipformerLangStatus.Installed(size)
            }
            _downloadState.value = DownloadState.Idle
            Log.i(TAG, "Downloaded ${lang.modelName} (${lang.modelFiles.size} files)")
        }
    }

    /** Download a language model. */
    suspend fun download(lang: MoonshineLanguage) {
        if (isInstalled(lang)) return

        withContext(Dispatchers.IO) {
            val dir = modelDir(lang)
            dir.mkdirs()

            val files = lang.modelFiles
            val fileWeight = 1f / files.size
            for ((idx, filename) in files.withIndex()) {
                val target = File(dir, filename)
                if (target.exists()) continue

                val baseProgress = idx.toFloat() / files.size
                val downloadingStatus = MoonshineLangStatus.Downloading(baseProgress)
                _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
                    it[lang] = downloadingStatus
                }
                _downloadState.value = DownloadState.Downloading(
                    progress = baseProgress,
                    currentFile = filename,
                    language = lang.displayName,
                )

                val url = "${lang.downloadBaseUrl}/$filename"
                try {
                    downloadMoonshineFile(lang, url, target, baseProgress, fileWeight)
                } catch (e: CancellationException) {
                    target.delete()
                    File(dir, "$filename.part").delete()
                    _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
                        it[lang] = MoonshineLangStatus.Missing
                    }
                    _downloadState.value = DownloadState.Idle
                    throw e
                } catch (e: Exception) {
                    target.delete()
                    File(dir, "$filename.part").delete()
                    val errorMsg = "Failed to download $filename: ${e.message}"
                    _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
                        it[lang] = MoonshineLangStatus.Error(errorMsg)
                    }
                    _downloadState.value = DownloadState.Error(errorMsg)
                    return@withContext
                }
            }

            val size = dir.walkTopDown().sumOf { if (it.isFile) it.length() else 0L }
            _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
                it[lang] = MoonshineLangStatus.Installed(size)
            }
            _downloadState.value = DownloadState.Idle
            Log.i(TAG, "Downloaded ${lang.modelName} (${files.size} files)")
        }
    }

    /** Delete a Zipformer language model. */
    fun deleteZipformer(lang: ZipformerLanguage) {
        zipformerDir(lang).deleteRecursively()
        _zipformerStatuses.value = _zipformerStatuses.value.toMutableMap().also {
            it[lang] = ZipformerLangStatus.Missing
        }
    }

    /** Delete a Moonshine language model. */
    fun deleteMoonshine(lang: MoonshineLanguage) {
        modelDir(lang).deleteRecursively()
        _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
            it[lang] = MoonshineLangStatus.Missing
        }
    }

    /** Delete a language model (legacy — updates status flow). */
    fun delete(lang: MoonshineLanguage) {
        deleteMoonshine(lang)
    }

    /**
     * Start a Zipformer download without blocking the caller.
     * The download runs on [managerScope] and survives dialog dismissal.
     * No-ops if already downloading or installed.
     */
    fun startDownloadZipformer(lang: ZipformerLanguage) {
        if (_zipformerStatuses.value[lang] is ZipformerLangStatus.Downloading) return
        managerScope.launch { downloadZipformer(lang) }
    }

    /**
     * Start a Moonshine download without blocking the caller.
     * The download runs on [managerScope] and survives dialog dismissal.
     * No-ops if already downloading or installed.
     */
    fun startDownloadMoonshine(lang: MoonshineLanguage) {
        if (_moonshineStatuses.value[lang] is MoonshineLangStatus.Downloading) return
        managerScope.launch { download(lang) }
    }

    /** Get total size of downloaded models. */
    fun installedSizeBytes(): Long {
        var total = 0L
        if (modelsRoot.exists()) {
            modelsRoot.walkTopDown().forEach { f -> if (f.isFile) total += f.length() }
        }
        return total
    }

    private suspend fun downloadMoonshineFile(
        lang: MoonshineLanguage,
        url: String,
        target: File,
        baseProgress: Float = 0f,
        fileWeight: Float = 1f,
    ) {
        val request = Request.Builder().url(url).build()
        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) throw Exception("HTTP ${response.code}")
            val body = response.body
            val contentLength = body.contentLength()
            var downloaded = 0L
            val tempTarget = File(target.parentFile, "${target.name}.part")
            tempTarget.delete()
            tempTarget.outputStream().use { out ->
                val buf = ByteArray(65536)
                val input = body.byteStream()
                while (true) {
                    kotlinx.coroutines.currentCoroutineContext().ensureActive()
                    val n = input.read(buf)
                    if (n < 0) break
                    out.write(buf, 0, n)
                    downloaded += n
                    if (contentLength > 0 && downloaded % (256 * 1024) < buf.size) {
                        val filePct = downloaded.toFloat() / contentLength
                        val overallProgress = baseProgress + filePct * fileWeight
                        _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
                            it[lang] = MoonshineLangStatus.Downloading(overallProgress)
                        }
                        _downloadState.value = DownloadState.Downloading(
                            progress = overallProgress,
                            currentFile = target.name,
                            language = lang.displayName,
                        )
                    }
                }
            }
            if (!tempTarget.renameTo(target)) {
                tempTarget.delete()
                throw Exception("Failed to finalize ${target.name}")
            }
        }
    }

    private suspend fun downloadZipformerFile(
        lang: ZipformerLanguage,
        url: String,
        target: File,
        baseProgress: Float = 0f,
        fileWeight: Float = 1f,
    ) {
        val request = Request.Builder().url(url).build()
        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) throw Exception("HTTP ${response.code}")
            val body = response.body
            val contentLength = body.contentLength()
            var downloaded = 0L
            val tempTarget = File(target.parentFile, "${target.name}.part")
            tempTarget.delete()
            tempTarget.outputStream().use { out ->
                val buf = ByteArray(65536)
                val input = body.byteStream()
                while (true) {
                    kotlinx.coroutines.currentCoroutineContext().ensureActive()
                    val n = input.read(buf)
                    if (n < 0) break
                    out.write(buf, 0, n)
                    downloaded += n
                    if (contentLength > 0 && downloaded % (256 * 1024) < buf.size) {
                        val filePct = downloaded.toFloat() / contentLength
                        val overallProgress = baseProgress + filePct * fileWeight
                        _zipformerStatuses.value = _zipformerStatuses.value.toMutableMap().also {
                            it[lang] = ZipformerLangStatus.Downloading(overallProgress)
                        }
                        _downloadState.value = DownloadState.Downloading(
                            progress = overallProgress,
                            currentFile = target.name,
                            language = lang.displayName,
                        )
                    }
                }
            }
            if (!tempTarget.renameTo(target)) {
                tempTarget.delete()
                throw Exception("Failed to finalize ${target.name}")
            }
        }
    }

    sealed class DownloadState {
        data object Idle : DownloadState()
        data class Downloading(
            val progress: Float,
            val currentFile: String,
            val language: String,
        ) : DownloadState()
        data class Error(val message: String) : DownloadState()
    }

    sealed class ZipformerLangStatus {
        data object Missing : ZipformerLangStatus()
        data class Downloading(val progress: Float) : ZipformerLangStatus()
        data class Installed(val sizeBytes: Long) : ZipformerLangStatus()
        data class Error(val message: String) : ZipformerLangStatus()
    }

    sealed class MoonshineLangStatus {
        data object Missing : MoonshineLangStatus()
        data class Downloading(val progress: Float) : MoonshineLangStatus()
        data class Installed(val sizeBytes: Long) : MoonshineLangStatus()
        data class Error(val message: String) : MoonshineLangStatus()
    }

    companion object {
        private const val TAG = "MoonshineModelManager"
    }
}

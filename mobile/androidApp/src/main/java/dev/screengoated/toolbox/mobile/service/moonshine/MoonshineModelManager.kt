package dev.screengoated.toolbox.mobile.service.moonshine

import android.content.Context
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
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
                _downloadState.value = DownloadState.Downloading(
                    progress = baseProgress,
                    currentFile = filename,
                    language = lang.displayName,
                )
                val url = "${lang.downloadBaseUrl}/$filename"
                try { downloadFile(url, target, baseProgress, fileWeight) } catch (e: Exception) {
                    target.delete()
                    _downloadState.value = DownloadState.Error("Failed to download $filename: ${e.message}")
                    return@withContext
                }
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
                _downloadState.value = DownloadState.Downloading(
                    progress = baseProgress,
                    currentFile = filename,
                    language = lang.displayName,
                )

                val url = "${lang.downloadBaseUrl}/$filename"
                try {
                    downloadFile(url, target, baseProgress, fileWeight)
                } catch (e: Exception) {
                    target.delete()
                    _downloadState.value = DownloadState.Error(
                        "Failed to download $filename: ${e.message}"
                    )
                    return@withContext
                }
            }

            _downloadState.value = DownloadState.Idle
            Log.i(TAG, "Downloaded ${lang.modelName} (${files.size} files)")
        }
    }

    /** Delete a language model. */
    fun delete(lang: MoonshineLanguage) {
        modelDir(lang).deleteRecursively()
    }

    /** Get total size of downloaded models. */
    fun installedSizeBytes(): Long {
        var total = 0L
        if (modelsRoot.exists()) {
            modelsRoot.walkTopDown().forEach { f -> if (f.isFile) total += f.length() }
        }
        return total
    }

    private fun downloadFile(url: String, target: File, baseProgress: Float = 0f, fileWeight: Float = 1f) {
        val request = Request.Builder().url(url).build()
        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) throw Exception("HTTP ${response.code}")
            val body = response.body ?: throw Exception("Empty response")
            val contentLength = body.contentLength()
            var downloaded = 0L
            target.outputStream().use { out ->
                val buf = ByteArray(65536)
                val input = body.byteStream()
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    out.write(buf, 0, n)
                    downloaded += n
                    if (contentLength > 0 && downloaded % (256 * 1024) < buf.size) {
                        val filePct = downloaded.toFloat() / contentLength
                        _downloadState.value = DownloadState.Downloading(
                            progress = baseProgress + filePct * fileWeight,
                            currentFile = target.name,
                            language = _downloadState.value.let { if (it is DownloadState.Downloading) it.language else "" },
                        )
                    }
                }
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

    companion object {
        private const val TAG = "MoonshineModelManager"
    }
}

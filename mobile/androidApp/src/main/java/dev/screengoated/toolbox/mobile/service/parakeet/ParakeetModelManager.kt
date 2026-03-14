package dev.screengoated.toolbox.mobile.service.parakeet

import android.content.Context
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.TimeUnit

class ParakeetModelManager(context: Context) {

    private val modelDir = File(context.filesDir, "models/parakeet")

    private val _state = MutableStateFlow<ParakeetModelState>(ParakeetModelState.Missing)
    val state: StateFlow<ParakeetModelState> = _state.asStateFlow()

    private val client = OkHttpClient.Builder()
        .connectTimeout(30, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .build()

    init {
        refreshState()
    }

    fun isInstalled(): Boolean {
        // Accept either int8 or fp32 encoder/decoder, plus tokenizer
        val hasEncoder = File(modelDir, "encoder.int8.onnx").exists() || File(modelDir, "encoder.onnx").exists()
        val hasDecoder = File(modelDir, "decoder_joint.int8.onnx").exists() || File(modelDir, "decoder_joint.onnx").exists()
        val hasTokenizer = File(modelDir, "tokenizer.json").exists()
        return hasEncoder && hasDecoder && hasTokenizer
    }

    fun refreshState() {
        _state.value = if (isInstalled()) {
            ParakeetModelState.Installed(installedSizeBytes())
        } else {
            ParakeetModelState.Missing
        }
    }

    private fun installedSizeBytes(): Long {
        if (!modelDir.exists()) return 0L
        return modelDir.listFiles()?.sumOf { it.length() } ?: 0L
    }

    suspend fun download() {
        if (isInstalled()) {
            refreshState()
            return
        }

        withContext(Dispatchers.IO) {
            modelDir.mkdirs()

            for (file in MODEL_FILES) {
                val target = File(modelDir, file.filename)
                if (target.exists()) continue

                _state.value = ParakeetModelState.Downloading(0f, file.filename)

                try {
                    downloadFile(file.url, target, file.filename)
                } catch (e: Exception) {
                    target.delete()
                    _state.value = ParakeetModelState.Error(
                        "Failed to download ${file.filename}: ${e.message}",
                    )
                    return@withContext
                }
            }

            refreshState()
        }
    }

    private suspend fun downloadFile(url: String, target: File, filename: String) {
        val request = Request.Builder()
            .url(url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Linux; Android) AppleWebKit/537.36 Chrome/91.0 Safari/537.36",
            )
            .build()

        val response = client.newCall(request).execute()
        if (!response.isSuccessful) {
            throw Exception("HTTP ${response.code}")
        }

        val body = response.body ?: throw Exception("Empty response body")
        val totalBytes = body.contentLength()
        val tmpFile = File(modelDir, "$filename.tmp")

        body.byteStream().use { input ->
            FileOutputStream(tmpFile).use { output ->
                val buffer = ByteArray(8192)
                var downloaded = 0L

                while (true) {
                    coroutineScope { ensureActive() }
                    val bytesRead = input.read(buffer)
                    if (bytesRead == -1) break

                    output.write(buffer, 0, bytesRead)
                    downloaded += bytesRead

                    if (totalBytes > 0) {
                        val progress = (downloaded.toFloat() / totalBytes) * 100f
                        _state.value = ParakeetModelState.Downloading(progress, filename)
                    }
                }
            }
        }

        tmpFile.renameTo(target)
    }

    fun delete() {
        _state.value = ParakeetModelState.Deleting
        modelDir.deleteRecursively()
        _state.value = ParakeetModelState.Missing
    }

    companion object {
        private const val TAG = "ParakeetModel"

        private const val BASE_URL =
            "https://huggingface.co/tteokl/parakeet-eou-120m-int8-onnx/resolve/main"

        private data class ModelFile(val filename: String, val url: String)

        private val MODEL_FILES = listOf(
            ModelFile("encoder.int8.onnx", "$BASE_URL/encoder.int8.onnx"),
            ModelFile("decoder_joint.int8.onnx", "$BASE_URL/decoder_joint.int8.onnx"),
            ModelFile("tokenizer.json", "$BASE_URL/tokenizer.json"),
        )
    }
}

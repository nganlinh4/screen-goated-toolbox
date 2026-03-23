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
import java.util.zip.ZipInputStream

class ParakeetModelManager(context: Context) {

    private val modelDir = File(context.filesDir, "models/parakeet")
    private val ortDir = File(context.filesDir, "ort_libs")

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
        return hasEncoder && hasDecoder && hasTokenizer && isOrtInstalled()
    }

    fun isOrtInstalled(): Boolean {
        return File(ortDir, "libonnxruntime.so").exists() &&
            File(ortDir, "libonnxruntime4j_jni.so").exists()
    }

    fun ortLibDir(): File = ortDir

    fun refreshState() {
        _state.value = if (isInstalled()) {
            ParakeetModelState.Installed(installedSizeBytes())
        } else {
            ParakeetModelState.Missing
        }
    }

    private fun installedSizeBytes(): Long {
        var total = 0L
        if (modelDir.exists()) total += modelDir.listFiles()?.sumOf { it.length() } ?: 0L
        if (ortDir.exists()) total += ortDir.listFiles()?.sumOf { it.length() } ?: 0L
        return total
    }

    suspend fun download() {
        if (isInstalled()) {
            refreshState()
            return
        }

        withContext(Dispatchers.IO) {
            // Download ORT native libs from Maven AAR (required for inference)
            if (!isOrtInstalled()) {
                try {
                    downloadOrtFromAar()
                } catch (e: Exception) {
                    _state.value = ParakeetModelState.Error(
                        "Failed to download ONNX Runtime: ${e.message}",
                    )
                    return@withContext
                }
            }

            // Download model files
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

    private suspend fun downloadOrtFromAar() {
        _state.value = ParakeetModelState.Downloading(0f, "ONNX Runtime")
        ortDir.mkdirs()

        val wantedEntries = ORT_FILES.associate { it.url.removePrefix("aar:") to it.filename }

        val request = Request.Builder()
            .url(ORT_AAR_URL)
            .header("User-Agent", "Mozilla/5.0 (Linux; Android) AppleWebKit/537.36 Chrome/91.0 Safari/537.36")
            .build()
        val response = client.newCall(request).execute()
        if (!response.isSuccessful) throw Exception("HTTP ${response.code}")
        val body = response.body ?: throw Exception("Empty response body")
        val totalBytes = body.contentLength()
        var bytesRead = 0L

        ZipInputStream(body.byteStream()).use { zip ->
            var entry = zip.nextEntry
            while (entry != null) {
                bytesRead += entry.compressedSize.coerceAtLeast(0)
                val outName = wantedEntries[entry.name]
                if (outName != null) {
                    val tmpFile = File(ortDir, "$outName.tmp")
                    FileOutputStream(tmpFile).use { out ->
                        zip.copyTo(out)
                    }
                    tmpFile.renameTo(File(ortDir, outName))
                    if (totalBytes > 0) {
                        _state.value = ParakeetModelState.Downloading(
                            (bytesRead.toFloat() / totalBytes) * 100f, "ONNX Runtime",
                        )
                    }
                }
                zip.closeEntry()
                entry = zip.nextEntry
            }
        }

        if (!isOrtInstalled()) {
            throw Exception("Failed to extract ORT libs from AAR")
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
        ortDir.deleteRecursively()
        _state.value = ParakeetModelState.Missing
    }

    companion object {
        private const val TAG = "ParakeetModel"

        private const val BASE_URL =
            "https://huggingface.co/tteokl/parakeet-eou-120m-int8-onnx/resolve/main"

        private const val ORT_VERSION = "1.24.2"
        private const val ORT_BASE_URL =
            "https://repo1.maven.org/maven2/com/microsoft/onnxruntime/onnxruntime-android/$ORT_VERSION"
        /** AAR is a ZIP — we extract arm64-v8a .so files from it. */
        internal const val ORT_AAR_URL = "$ORT_BASE_URL/onnxruntime-android-$ORT_VERSION.aar"

        private data class ModelFile(val filename: String, val url: String)

        private val MODEL_FILES = listOf(
            ModelFile("encoder.int8.onnx", "$BASE_URL/encoder.int8.onnx"),
            ModelFile("decoder_joint.int8.onnx", "$BASE_URL/decoder_joint.int8.onnx"),
            ModelFile("tokenizer.json", "$BASE_URL/tokenizer.json"),
        )

        private val ORT_FILES = listOf(
            ModelFile("libonnxruntime.so", "aar:jni/arm64-v8a/libonnxruntime.so"),
            ModelFile("libonnxruntime4j_jni.so", "aar:jni/arm64-v8a/libonnxruntime4j_jni.so"),
        )
    }
}

package dev.screengoated.toolbox.mobile.service.moonshine

import android.content.Context
import android.util.Log
import dev.screengoated.toolbox.mobile.service.nativelibs.RuntimeLeaseRegistry
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
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
class MoonshineModelManager private constructor(context: Context) {

    private val context = context.applicationContext

    private sealed interface ModelKey {
        data class Moonshine(val language: MoonshineLanguage) : ModelKey
        data class Zipformer(val language: ZipformerLanguage) : ModelKey
    }

    private val modelsRoot = File(this.context.filesDir, "models/moonshine")
    private val moonshineBundles = MoonshineModelDelivery.load(this.context)
    private val leases = RuntimeLeaseRegistry<ModelKey>(::finishRemoval)
    private val downloadLocks = buildMap<ModelKey, Mutex> {
        MoonshineLanguage.entries.forEach { put(ModelKey.Moonshine(it), Mutex()) }
        ZipformerLanguage.entries.forEach { put(ModelKey.Zipformer(it), Mutex()) }
    }

    private val _downloadState = MutableStateFlow<DownloadState>(DownloadState.Idle)
    val downloadState: StateFlow<DownloadState> = _downloadState.asStateFlow()

    private val _zipformerStatuses = MutableStateFlow(
        ZipformerLanguage.entries.associateWith { lang ->
            if (isZipformerPayloadPresent(lang)) {
                ZipformerLangStatus.Installed(lang.modelFileContracts.sumOf { it.byteCount })
            } else {
                ZipformerLangStatus.Missing
            }
        }
    )
    val zipformerStatuses: StateFlow<Map<ZipformerLanguage, ZipformerLangStatus>> = _zipformerStatuses.asStateFlow()

    private val _moonshineStatuses = MutableStateFlow(
        MoonshineLanguage.entries.associateWith { lang ->
            if (isMoonshinePayloadPresent(lang)) {
                MoonshineLangStatus.Installed(lang.expectedSizeBytes)
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
    private val bundleInstaller = MoonshineBundleInstaller(
        client,
        File(this.context.cacheDir, "moonshine-model-bundles"),
    )
    private val removalPreferences = this.context.getSharedPreferences(
        REMOVAL_PREFERENCES,
        Context.MODE_PRIVATE,
    )

    init {
        MoonshineLanguage.entries
            .map(ModelKey::Moonshine)
            .filter(::isRemovalPersisted)
            .forEach(leases::requestRemoval)
        ZipformerLanguage.entries
            .map(ModelKey::Zipformer)
            .filter(::isRemovalPersisted)
            .forEach(leases::requestRemoval)
    }

    /** Check if a language model is downloaded and ready. */
    fun isInstalled(lang: MoonshineLanguage): Boolean =
        MoonshineModelIntegrity.verified(modelDir(lang), lang.modelFileContracts)

    fun isMoonshinePayloadPresent(lang: MoonshineLanguage): Boolean =
        MoonshineModelIntegrity.payloadPresent(modelDir(lang), lang.modelFileContracts)

    /** Get the filesystem path for a Moonshine model directory. */
    fun modelDir(lang: MoonshineLanguage): File = File(modelsRoot, lang.modelName)

    /** Get the filesystem path for a Zipformer model directory. */
    fun zipformerDir(lang: ZipformerLanguage): File = File(modelsRoot, lang.modelName)

    fun isZipformerPayloadPresent(lang: ZipformerLanguage): Boolean =
        ZipformerModelIntegrity.payloadPresent(zipformerDir(lang), lang.modelFileContracts)

    fun isZipformerInstalled(lang: ZipformerLanguage): Boolean {
        val dir = zipformerDir(lang)
        return dir.exists() && ZipformerModelIntegrity.verified(dir, lang.modelFileContracts)
    }

    suspend fun downloadZipformer(lang: ZipformerLanguage) {
        val key = ModelKey.Zipformer(lang)
        downloadLocks.getValue(key).withLock {
            val lease = leases.acquire(listOf(key)) ?: return@withLock
            try {
                if (isZipformerInstalled(lang)) return@withLock
                withContext(Dispatchers.IO) {
            val dir = zipformerDir(lang)
            dir.mkdirs()

            val files = lang.modelFileContracts
            val fileWeight = 1f / files.size
            for ((idx, file) in files.withIndex()) {
                val target = File(dir, file.name)
                if (ZipformerModelIntegrity.verified(target, file)) continue
                val baseProgress = idx.toFloat() / files.size
                val downloadingStatus = ZipformerLangStatus.Downloading(baseProgress)
                _zipformerStatuses.value = _zipformerStatuses.value.toMutableMap().also {
                    it[lang] = downloadingStatus
                }
                _downloadState.value = DownloadState.Downloading(
                    progress = baseProgress,
                    currentFile = file.name,
                    language = lang.displayName,
                )
                val url = "${lang.downloadBaseUrl}/${file.name}"
                try {
                    downloadZipformerFile(lang, file, url, target, baseProgress, fileWeight)
                } catch (e: CancellationException) {
                    File(dir, "${file.name}.part").delete()
                    _zipformerStatuses.value = _zipformerStatuses.value.toMutableMap().also {
                        it[lang] = ZipformerLangStatus.Missing
                    }
                    _downloadState.value = DownloadState.Idle
                    throw e
                } catch (e: Exception) {
                    File(dir, "${file.name}.part").delete()
                    val errorMsg = "Failed to download ${file.name}: ${e.message}"
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
            Log.i(TAG, "Downloaded ${lang.modelName} (${files.size} files)")
                }
            } finally {
                lease.close()
            }
        }
    }

    /** Download a language model. */
    suspend fun download(lang: MoonshineLanguage) {
        val key = ModelKey.Moonshine(lang)
        downloadLocks.getValue(key).withLock {
            val lease = leases.acquire(listOf(key)) ?: return@withLock
            try {
                if (isInstalled(lang)) return@withLock
                withContext(Dispatchers.IO) {
            val dir = modelDir(lang)
            dir.mkdirs()

            val files = lang.modelFileContracts
            val bundleInstalled = try {
                bundleInstaller.install(
                    lang,
                    moonshineBundles.getValue(lang.modelName),
                    dir,
                ) { progress, currentFile ->
                    updateMoonshineProgress(lang, progress, currentFile)
                }
                true
            } catch (e: CancellationException) {
                _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
                    it[lang] = MoonshineLangStatus.Missing
                }
                _downloadState.value = DownloadState.Idle
                throw e
            } catch (e: Exception) {
                Log.w(TAG, "Primary model bundle unavailable; using verified file transport", e)
                false
            }
            val fallbackFiles = if (bundleInstalled) emptyList() else files
            val fileWeight = 1f / files.size
            for ((idx, file) in fallbackFiles.withIndex()) {
                val target = File(dir, file.name)
                if (MoonshineModelIntegrity.verified(target, file)) continue

                val baseProgress = idx.toFloat() / files.size
                val downloadingStatus = MoonshineLangStatus.Downloading(baseProgress)
                _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
                    it[lang] = downloadingStatus
                }
                _downloadState.value = DownloadState.Downloading(
                    progress = baseProgress,
                    currentFile = file.name,
                    language = lang.displayName,
                )

                val url = "${lang.downloadBaseUrl}/${file.name}"
                try {
                    downloadMoonshineFile(lang, file, url, target, baseProgress, fileWeight)
                } catch (e: CancellationException) {
                    File(dir, "${file.name}.part").delete()
                    _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
                        it[lang] = MoonshineLangStatus.Missing
                    }
                    _downloadState.value = DownloadState.Idle
                    throw e
                } catch (e: Exception) {
                    File(dir, "${file.name}.part").delete()
                    val errorMsg = "Failed to download ${file.name}: ${e.message}"
                    _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
                        it[lang] = MoonshineLangStatus.Error(errorMsg)
                    }
                    _downloadState.value = DownloadState.Error(errorMsg)
                    return@withContext
                }
            }

            check(MoonshineModelIntegrity.verified(dir, files)) {
                "Downloaded ${lang.modelName} failed integrity verification"
            }
            _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
                it[lang] = MoonshineLangStatus.Installed(lang.expectedSizeBytes)
            }
            _downloadState.value = DownloadState.Idle
            Log.i(TAG, "Downloaded ${lang.modelName} (${files.size} files)")
                }
            } finally {
                lease.close()
            }
        }
    }

    /** Delete a Zipformer language model. */
    fun deleteZipformer(lang: ZipformerLanguage) {
        persistRemoval(ModelKey.Zipformer(lang), true)
        _zipformerStatuses.value = _zipformerStatuses.value.toMutableMap().also {
            it[lang] = ZipformerLangStatus.RemovalPending(REMOVAL_PENDING_MESSAGE)
        }
        leases.requestRemoval(ModelKey.Zipformer(lang))
    }

    /** Delete a Moonshine language model. */
    fun deleteMoonshine(lang: MoonshineLanguage) {
        persistRemoval(ModelKey.Moonshine(lang), true)
        _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
            it[lang] = MoonshineLangStatus.RemovalPending(REMOVAL_PENDING_MESSAGE)
        }
        leases.requestRemoval(ModelKey.Moonshine(lang))
    }

    fun acquireMoonshine(lang: MoonshineLanguage): AutoCloseable? =
        if (isInstalled(lang)) leases.acquire(listOf(ModelKey.Moonshine(lang))) else null

    fun acquireZipformer(lang: ZipformerLanguage): AutoCloseable? =
        if (isZipformerInstalled(lang)) leases.acquire(listOf(ModelKey.Zipformer(lang))) else null

    private fun finishRemoval(key: ModelKey) {
        when (key) {
            is ModelKey.Moonshine -> finishMoonshineRemoval(key.language, key)
            is ModelKey.Zipformer -> finishZipformerRemoval(key.language, key)
        }
    }

    private fun finishMoonshineRemoval(lang: MoonshineLanguage, key: ModelKey) {
        val removed = !modelDir(lang).exists() || MoonshineModelIntegrity.removeManagedFiles(
            modelDir(lang),
            lang.modelFileContracts,
        )
        _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
            it[lang] = if (removed) {
                persistRemoval(key, false)
                leases.completeRemoval(key)
                MoonshineLangStatus.Missing
            } else {
                MoonshineLangStatus.RemovalPending(
                    REMOVAL_FAILED_MESSAGE,
                    retryable = true,
                )
            }
        }
    }

    private fun finishZipformerRemoval(lang: ZipformerLanguage, key: ModelKey) {
        val removed = !zipformerDir(lang).exists() || ZipformerModelIntegrity.removeManagedFiles(
            zipformerDir(lang),
            lang.modelFileContracts,
        )
        _zipformerStatuses.value = _zipformerStatuses.value.toMutableMap().also {
            it[lang] = if (removed) {
                persistRemoval(key, false)
                leases.completeRemoval(key)
                ZipformerLangStatus.Missing
            } else {
                ZipformerLangStatus.RemovalPending(
                    REMOVAL_FAILED_MESSAGE,
                    retryable = true,
                )
            }
        }
    }

    private fun isRemovalPersisted(key: ModelKey): Boolean =
        removalPreferences.getBoolean(removalKey(key), false)

    private fun persistRemoval(key: ModelKey, pending: Boolean) {
        removalPreferences.edit().run {
            if (pending) putBoolean(removalKey(key), true) else remove(removalKey(key))
        }.apply()
    }

    private fun removalKey(key: ModelKey): String = when (key) {
        is ModelKey.Moonshine -> "moonshine_${key.language.name.lowercase()}"
        is ModelKey.Zipformer -> "zipformer_${key.language.name.lowercase()}"
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
        if (_zipformerStatuses.value[lang] is ZipformerLangStatus.Downloading ||
            leases.isRemovalPending(ModelKey.Zipformer(lang))
        ) return
        managerScope.launch { downloadZipformer(lang) }
    }

    /**
     * Start a Moonshine download without blocking the caller.
     * The download runs on [managerScope] and survives dialog dismissal.
     * No-ops if already downloading or installed.
     */
    fun startDownloadMoonshine(lang: MoonshineLanguage) {
        if (_moonshineStatuses.value[lang] is MoonshineLangStatus.Downloading ||
            leases.isRemovalPending(ModelKey.Moonshine(lang))
        ) return
        managerScope.launch { download(lang) }
    }

    /** Get total size of downloaded models. */
    fun installedSizeBytes(): Long =
        MoonshineLanguage.entries.sumOf { lang ->
            if (isMoonshinePayloadPresent(lang)) lang.expectedSizeBytes else 0L
        } + ZipformerLanguage.entries.sumOf { lang ->
            if (isZipformerPayloadPresent(lang)) {
                lang.modelFileContracts.sumOf { it.byteCount }
            } else {
                0L
            }
        }

    private fun updateMoonshineProgress(
        lang: MoonshineLanguage,
        progress: Float,
        currentFile: String,
    ) {
        _moonshineStatuses.value = _moonshineStatuses.value.toMutableMap().also {
            it[lang] = MoonshineLangStatus.Downloading(progress)
        }
        _downloadState.value = DownloadState.Downloading(progress, currentFile, lang.displayName)
    }

    private suspend fun downloadMoonshineFile(
        lang: MoonshineLanguage,
        contract: MoonshineModelFile,
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
            if (contentLength >= 0 && contentLength != contract.byteCount) {
                throw Exception("Download size for ${contract.name} does not match this build")
            }
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
                    if (downloaded + n > contract.byteCount) {
                        throw Exception("Download for ${contract.name} exceeds its limit")
                    }
                    out.write(buf, 0, n)
                    downloaded += n
                    if (downloaded % (256 * 1024) < buf.size) {
                        val filePct = downloaded.toFloat() / contract.byteCount
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
            MoonshineModelIntegrity.finalizeVerifiedPart(tempTarget, target, contract)
        }
    }

    private suspend fun downloadZipformerFile(
        lang: ZipformerLanguage,
        contract: ZipformerModelFile,
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
            if (contentLength >= 0 && contentLength != contract.byteCount) {
                throw Exception("Download size for ${contract.name} does not match this build")
            }
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
                    if (downloaded + n > contract.byteCount) {
                        throw Exception("Download for ${contract.name} exceeds its limit")
                    }
                    out.write(buf, 0, n)
                    downloaded += n
                    if (downloaded % (256 * 1024) < buf.size) {
                        val filePct = downloaded.toFloat() / contract.byteCount
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
            ZipformerModelIntegrity.finalizeVerifiedPart(tempTarget, target, contract)
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
        data class RemovalPending(
            val message: String,
            val retryable: Boolean = false,
        ) : ZipformerLangStatus()
        data class Error(val message: String) : ZipformerLangStatus()
    }

    sealed class MoonshineLangStatus {
        data object Missing : MoonshineLangStatus()
        data class Downloading(val progress: Float) : MoonshineLangStatus()
        data class Installed(val sizeBytes: Long) : MoonshineLangStatus()
        data class RemovalPending(
            val message: String,
            val retryable: Boolean = false,
        ) : MoonshineLangStatus()
        data class Error(val message: String) : MoonshineLangStatus()
    }

    companion object {
        private const val TAG = "MoonshineModelManager"
        private const val REMOVAL_PREFERENCES = "downloaded_model_lifecycle"
        private const val REMOVAL_PENDING_MESSAGE = "Removal pending until the active session stops."
        private const val REMOVAL_FAILED_MESSAGE = "Removal could not finish. Restart or try again."

        @Volatile
        private var instance: MoonshineModelManager? = null

        fun get(context: Context): MoonshineModelManager = instance ?: synchronized(this) {
            instance ?: MoonshineModelManager(context.applicationContext).also { instance = it }
        }

        fun reconcilePendingRemovals(context: Context) {
            val preferences = context.applicationContext.getSharedPreferences(
                REMOVAL_PREFERENCES,
                Context.MODE_PRIVATE,
            )
            if (preferences.all.values.any { it == true }) get(context)
        }
    }
}

package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.database.Cursor
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.os.StatFs
import java.io.File
import java.io.InputStream
import java.security.MessageDigest
import java.util.UUID

internal class CreationFileStore(internal val context: Context) {
    private val resolver = context.contentResolver
    private val recentManagedPaths = CreationRecentManagedPaths()
    private val previewCache = CreationPreviewCache()
    internal val outputs = CreationOutputStore(context, recentManagedPaths::remember)
    private val sourceHandles = CreationSourceHandleLeases()
    private val sourceHandleLock = Any()
    internal val uriGrants = CreationUriGrantLedger(context)
    private val jobInputs by lazy {
        CreationJobInputStore(
            File(context.filesDir, "creation/job-inputs"),
            ::openInput,
        ) { source, target -> linkCreationAcceptedInput(context.filesDir, source, target) }
    }
    private val presentationPreviews by lazy {
        CreationPresentationPreviewStore(
            File(context.filesDir, "creation/presentation"),
            ::openInput,
        )
    }
    private val pendingCleanup by lazy { CreationPendingCleanupStore(context, this) }
    private val jobInputCleanup by lazy { CreationJobInputCleanupStore(context) }

    fun importImages(
        uris: List<Uri>,
        tool: CreationTool,
        maximumBatchImages: Int,
        existingReferencePaths: List<String> = emptyList(),
    ): List<String> = synchronized(creationImportAdmissionLock) {
        require(uris.isNotEmpty()) { "Choose at least one image" }
        require(
            maximumBatchImages > 0 &&
                uris.size <= minOf(maximumBatchImages, CreationContract.MAXIMUM_PICKER_BATCH_IMAGES),
        ) { "Too many images were selected" }
        val validationReservation = uris.fold(0L) { total, uri ->
            creationSaturatingBytes(
                total,
                query(uri, OpenableColumns.SIZE)?.toLongOrNull()
                    ?.coerceIn(0L, CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES)
                    ?: CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES,
            )
        }
        ensureCreationStorageAvailable(
            tool,
            validationReservation,
            outputDestinationSnapshot(),
        )
        val sourceDir = File(context.filesDir, "creation/sources").apply { mkdirs() }
        val persistentHandles = uris.associateWith(uriGrants::persistSource)
        val fallbackCount = persistentHandles.count { !it.value }
        val fallbackReservation = persistentHandles.entries.sumOf { (uri, persistent) ->
            if (persistent) 0L else {
                query(uri, OpenableColumns.SIZE)?.toLongOrNull()
                    ?.coerceIn(0L, CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES)
                    ?: CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES
            }
        }
        pruneDirectory(
            sourceDir,
            (MAXIMUM_SOURCE_FILES - fallbackCount).coerceAtLeast(0),
            SOURCE_RETENTION_MS,
            (MAXIMUM_SOURCE_BYTES - fallbackReservation).coerceAtLeast(0L),
            emptySet(),
        )
        creationRegularFilesNoFollow(sourceDir)
            .filter { it.name.endsWith(".pending") }
            .forEach { deleteCreationFileConfined(sourceDir, it) }
        val existingSources = creationRegularFilesNoFollow(sourceDir)
        require(existingSources.size + fallbackCount <= MAXIMUM_SOURCE_FILES) {
            "The image cache is full"
        }
        val existingBytes = existingSources.sumOf { it.length().coerceAtLeast(0L) }
        val existingReferenceBytes = if (tool == CreationTool.IMAGE_CREATOR) {
            existingReferencePaths.fold(0L) { total, path ->
                val sourceBytes = size(path)
                require(sourceBytes >= 0L) { "A reference image is unavailable" }
                creationSaturatingBytes(total, sourceBytes)
            }
        } else {
            0L
        }
        val importBudget = (
            if (tool == CreationTool.IMAGE_CREATOR) {
                CreationContract.MAXIMUM_IMAGE_REFERENCE_AGGREGATE_BYTES -
                    existingReferenceBytes
            } else {
                CreationContract.MAXIMUM_PICKER_AGGREGATE_BYTES
            }
        ).coerceAtLeast(0L)
        require(importBudget > 0L) { "Reference images reached the size limit" }
        require(existingBytes < MAXIMUM_SOURCE_BYTES) { "The image cache is full" }
        val imported = mutableListOf<String>()
        var aggregateBytes = 0L
        var fallbackBytes = 0L
        try {
            uris.forEach { uri ->
                val original = displayName(uri) ?: "image"
                val declared = query(uri, OpenableColumns.SIZE)?.toLongOrNull()
                val remainingImportBytes = importBudget - aggregateBytes
                val persistent = persistentHandles[uri] == true
                val remainingCacheBytes = if (persistent) {
                    CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES
                } else {
                    MAXIMUM_SOURCE_BYTES - existingBytes - fallbackBytes
                }
                val remainingBytes = minOf(
                    CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES,
                    remainingImportBytes,
                    remainingCacheBytes,
                )
                require(remainingBytes > 0L) {
                    "Selected images exceed the available import space"
                }
                require(
                    declared == null ||
                        declared in 0..CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES,
                ) {
                    "An image is larger than 25 MiB"
                }
                require(
                    declared == null ||
                        declared <= remainingBytes,
                ) { "Selected images exceed the available import space" }
                val safe = safeCreationManagedName(original)
                val target = uniqueCreationManagedFile(sourceDir, safe)
                val pending = File(sourceDir, "${target.name}.${UUID.randomUUID()}.pending")
                val copied = resolver.openInputStream(uri).use { input ->
                    requireNotNull(input) { "Could not open $original" }
                    copyCreationInputBounded(
                        input,
                        pending,
                        remainingBytes,
                    )
                }
                validateImportedCreationImage(pending)
                aggregateBytes += copied
                if (persistent) {
                    check(pending.delete()) { "Could not release the validation copy" }
                    imported += uri.toString()
                } else {
                    check(pending.renameTo(target)) { "Could not finish importing $original" }
                    imported += target.absolutePath
                    fallbackBytes += copied
                    recentManagedPaths.remember(target)
                }
            }
        } catch (error: Throwable) {
            imported.filterNot { it.startsWith("content://") }.forEach { File(it).delete() }
            sourceDir.listFiles { file -> file.name.endsWith(".pending") }
                ?.forEach(File::delete)
            reconcilePersistedUriGrants()
            throw error
        }
        pruneDirectory(
            sourceDir,
            MAXIMUM_SOURCE_FILES,
            SOURCE_RETENTION_MS,
            MAXIMUM_SOURCE_BYTES,
            imported.filterNot { it.startsWith("content://") }.toSet(),
        )
        imported
    }
    fun rememberOutputDirectory(uri: Uri): String = uriGrants.persistOutput(uri)
        .also { require(it) }.let { outputs.rememberDirectory(uri) }
        .also { reconcilePersistedUriGrants() }
    fun defaultOutputDirectoryLabel(): String = outputs.defaultDirectoryLabel()
    fun stagingFile(tool: CreationTool, sourcePath: String, extension: String): File {
        val directory = File(context.filesDir, "creation/staging/${tool.wireName}").apply { mkdirs() }
        val stem = File(sourcePath).nameWithoutExtension.ifBlank { tool.wireName }
        return reserveCreationStagingFile(
            directory,
            "${safeCreationManagedStem(stem)}.$extension",
        ).also { file ->
            recentManagedPaths.remember(file)
            pruneDirectory(
                directory,
                MAXIMUM_STAGING_FILES,
                STAGING_RETENTION_MS,
                MAXIMUM_STAGING_BYTES,
                setOf(file.absolutePath),
            )
        }
    }

    fun sealReservedStagingFile(tool: CreationTool, path: String): File =
        sealReservedCreationStagingFile(context.filesDir, tool, path)
    fun deleteReservedStagingFile(tool: CreationTool, path: String): Boolean =
        deleteReservedCreationStagingFile(context.filesDir, tool, path)
    fun outputDestinationSnapshot(): String? = outputs.destinationSnapshot()

    fun exists(path: String): Boolean = when (val uri = path.creationUriOrNull()) {
        null -> File(path).isFile
        else -> runCatching {
            resolver.openAssetFileDescriptor(uri, "r")?.use { true } ?: false
        }.getOrDefault(false)
    }

    fun size(path: String): Long = when (val uri = path.creationUriOrNull()) {
        null -> File(path).length()
        else -> query(uri, OpenableColumns.SIZE)?.toLongOrNull() ?: -1L
    }

    fun sha256(path: String): String {
        val digest = MessageDigest.getInstance("SHA-256")
        openInput(path).use { input ->
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                digest.update(buffer, 0, read)
            }
        }
        return digest.digest().joinToString("") { byte -> "%02x".format(byte) }
    }

    fun isManagedPath(path: String): Boolean {
        if (isUserOwnedCreationOutputPath(path)) return false
        val candidate = runCatching { File(path).canonicalFile }.getOrNull() ?: return false
        return managedRoots().any { root ->
            candidate.path.startsWith("${root.path}${File.separator}")
        }
    }

    fun managedPathIdentity(path: String): String? {
        if (!isManagedPath(path)) return null
        return runCatching { File(path).canonicalPath }.getOrNull()
    }

    fun journalProtectedManagedPaths(extra: Set<String>): Set<String> =
        (extra + creationJournalProtectedPaths(context.filesDir))
            .mapNotNull(::managedPathIdentity)
            .toSet()

    fun deleteManagedPath(path: String): Boolean {
        val identity = managedPathIdentity(path) ?: return false
        val file = File(identity)
        val root = managedRoots().firstOrNull { candidate ->
            file.toPath().toAbsolutePath().normalize().startsWith(candidate.toPath())
        } ?: return false
        return !file.exists() || deleteCreationFileConfined(root, file)
    }

    fun pendingCleanupStore(): CreationPendingCleanupStore = pendingCleanup
    fun updateSurfaceSources(ownerId: String, paths: Set<String>) {
        val released = synchronized(sourceHandleLock) { sourceHandles.update(ownerId, paths) }
        released.forEach(::deleteManagedPath)
        reconcilePersistedUriGrants()
    }

    fun releaseSurfaceSources(ownerId: String) {
        val released = synchronized(sourceHandleLock) { sourceHandles.release(ownerId) }
        released.forEach(::deleteManagedPath)
        reconcilePersistedUriGrants()
    }
    internal fun leasedSourceHandles(): Set<String> =
        synchronized(sourceHandleLock, sourceHandles::all)
    fun materializeJobInputs(
        ownerId: String,
        jobId: String,
        sourceHandles: List<String>,
        tool: CreationTool,
        destination: String?,
    ): List<String> {
        val leased = synchronized(sourceHandleLock) {
            this@CreationFileStore.sourceHandles.paths(ownerId)
        }
        if (!creationGenerationSourcesAreUsable(context.filesDir, leased, sourceHandles, ::exists)) {
            throw CreationSourceUnavailableException()
        }
        val maximumSourceBytes = if (tool == CreationTool.IMAGE_CREATOR) {
            CreationContract.MAXIMUM_IMAGE_REFERENCE_AGGREGATE_BYTES
        } else {
            CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES
        }
        val sourceSnapshotBytes = sourceHandles.fold(0L) { total, path ->
            if (creationAcceptedInputCanLink(context.filesDir, path)) return@fold total
            val observed = size(path).takeIf { it >= 0L }
                ?: CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES
            creationSaturatingBytes(
                total,
                minOf(observed, CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES),
            )
        }.coerceAtMost(maximumSourceBytes)
        ensureCreationStorageAvailable(tool, sourceSnapshotBytes, destination)
        return jobInputs.materialize(jobId, sourceHandles, tool)
    }

    fun releaseJobInputs(paths: List<String>): Boolean = jobInputs.release(paths)
    fun queueJobInputCleanup(paths: Collection<String>) = jobInputCleanup.record(paths)
    fun drainJobInputCleanup() = jobInputCleanup.drain()

    fun restoredRequestIsValid(request: CreationWorkerRequest): Boolean = validateRestoredCreationRequest(
        context.filesDir, request, ::size, ::sha256,
    )

    fun planManagedIsolation(path: String): CreationFileIsolation? {
        val identity = managedPathIdentity(path) ?: return null
        val file = File(identity)
        val root = managedRoot(file) ?: return null
        return planCreationFileIsolation(root, file)
    }

    fun isolateManagedPath(isolation: CreationFileIsolation): Boolean {
        val root = managedRoot(isolation.original) ?: return false
        return isolateCreationFileConfined(root, isolation)
    }

    fun isolateManagedPathIfIdentity(
        isolation: CreationFileIsolation,
        expectedIdentity: String?,
    ): Boolean {
        if (expectedIdentity == null ||
            artifactIdentity(isolation.original.absolutePath) != expectedIdentity
        ) return false
        if (!isolateManagedPath(isolation)) return false
        return artifactIdentity(isolation.isolated.absolutePath) == expectedIdentity
    }

    fun deleteManagedPathIfIdentity(path: String, expectedIdentity: String): Boolean =
        artifactIdentity(path) == expectedIdentity && deleteManagedPath(path)

    fun restoreManagedPath(isolation: CreationFileIsolation): Boolean {
        val root = managedRoot(isolation.original) ?: return false
        return restoreCreationFileConfined(root, isolation)
    }

    fun planRelinquishedManagedPath(isolation: CreationFileIsolation): String? {
        val root = managedRoot(isolation.original) ?: return null
        return planCreationRelinquishedFile(root, isolation)?.absolutePath
    }

    fun resolveManagedIsolation(
        isolation: CreationFileIsolation,
        replacementPath: String,
    ): Boolean {
        if (replacementPath == isolation.original.absolutePath) {
            return restoreManagedPath(isolation)
        }
        val root = managedRoot(isolation.original) ?: return false
        return relinquishCreationFileConfined(root, isolation, File(replacementPath))
    }

    fun pruneManagedArtifacts(protectedPaths: Set<String>, budgetBytes: Long) {
        if (!creationDurableStateIsReadable(context.filesDir)) {
            throw CreationStorageUnavailableException()
        }
        pruneCreationManagedArtifacts(
            roots = managedRoots(),
            libraryRoot = File(context.filesDir, "creation/library"),
            protectedPaths = protectedPaths + durableProtectedPaths(),
            recentPaths = recentManagedPaths.protectedPaths(),
            budgetBytes = budgetBytes,
            cleanup = pendingCleanup,
        )
        val snapshot = snapshotCreationManagedStorage(
            managedRoots(),
            protectedPaths + durableProtectedPaths(),
        )
        require(snapshot.totalBytes <= budgetBytes) { CREATION_STORAGE_UNAVAILABLE_ERROR_KEY }
    }

    fun managedStorageBytes(): Long =
        snapshotCreationManagedStorage(managedRoots(), emptySet()).totalBytes

    private fun ensureCreationStorageAvailable(
        tool: CreationTool,
        sourceSnapshotBytes: Long,
        destination: String?,
    ) {
        reconcileJobInputOwnership()
        val resultBytes = maximumCreationResultBytes(tool)
        val pending = creationPendingStorageReservations(
            context.filesDir,
            ::size,
        )
        val requirements = creationStorageRequirements(
            sourceSnapshotBytes,
            resultBytes,
            managedDestination = destination == null,
            pendingInternalBytes = pending.internalBytes,
            pendingDestinationBytes = destination?.let {
                pending.destinationBytes.getOrDefault(it, 0L)
            } ?: 0L,
        )
        val protected = durableProtectedPaths()
        val roots = managedRoots()
        val before = snapshotCreationManagedStorage(roots, protected)
        val initial = planCreationStorageAdmission(
            before.totalBytes,
            before.protectedBytes,
            availableCreationStorageBytes(),
            requirements.internalBytes,
        )
        val pressureBudget = creationPressurePruneBudget(
            before.totalBytes,
            availableCreationStorageBytes(),
            initial.requiredAvailableBytes,
            initial.pruneBudgetBytes,
        )
        if (before.protectedBytes > pressureBudget) {
            throw CreationStorageUnavailableException()
        }
        CreationHistoryStore(context, this).maintain(
            budgetBytes = pressureBudget,
            pruneEphemeral = false,
        )
        pruneManagedArtifacts(emptySet(), pressureBudget)
        val after = snapshotCreationManagedStorage(roots, protected)
        val final = planCreationStorageAdmission(
            after.totalBytes,
            after.protectedBytes,
            availableCreationStorageBytes(),
            requirements.internalBytes,
        )
        if (!final.accepted) throw CreationStorageUnavailableException()
        if (destination != null) {
            if (!creationExternalStorageAccepted(
                    outputs.availableBytes(destination),
                    requirements.destinationBytes,
                )
            ) {
                throw CreationStorageUnavailableException()
            }
        }
    }

    private fun availableCreationStorageBytes(): Long =
        runCatching { StatFs(context.filesDir.absolutePath).availableBytes }
            .getOrElse { throw CreationStorageUnavailableException() }

    fun readBytes(path: String, maximum: Long): ByteArray {
        val knownSize = size(path)
        require(knownSize < 0 || knownSize <= maximum) { "Preview asset is too large" }
        val input = path.creationUriOrNull()?.let(resolver::openInputStream)
            ?: File(path).inputStream()
        return input.use { stream -> readCreationBytesBounded(stream, maximum) }
    }

    fun writeText(path: String, value: String) {
        val uri = path.creationUriOrNull()
        if (uri == null) {
            File(path).writeText(value)
        } else {
            requireNotNull(resolver.openOutputStream(uri, "wt")) { "Result is not writable" }
                .bufferedWriter()
                .use { it.write(value) }
        }
    }

    fun delete(path: String): Boolean = outputs.delete(path)
    fun openExternally(path: String) = outputs.openExternally(path)

    fun openInput(path: String): InputStream =
        path.creationUriOrNull()?.let(resolver::openInputStream)
        ?: File(path).inputStream()

    fun uploadUri(path: String): Uri = outputs.uploadUri(path)

    fun presentationHandle(path: String): String =
        if (path.startsWith("content://")) path else presentationPreviews.materialize(path)

    fun prunePresentationArtifacts() = pruneCreationPresentationArtifacts(
        context.filesDir,
        pendingCleanup,
    )

    fun materializePreview(path: String, extension: String): File {
        val safeExtension = extension.lowercase().filter(Char::isLetterOrDigit).ifBlank { "bin" }
        val maximumBytes = creationPreviewMaximumBytes(safeExtension)
        val uri = path.creationUriOrNull()
        if (uri == null) {
            val local = File(path)
            require(local.length() <= maximumBytes) { "Preview asset is too large" }
            validateCreationPreviewArtifact(local, safeExtension)
            recentManagedPaths.remember(local)
            return local
        }
        val directory = File(context.cacheDir, "creation/previews").apply { mkdirs() }
        val expectedSize = size(path)
        require(expectedSize < 0L || expectedSize <= maximumBytes) { "Preview asset is too large" }
        val modified = query(uri, DocumentsContract.Document.COLUMN_LAST_MODIFIED)
            ?: query(uri, MediaStore.MediaColumns.DATE_MODIFIED)
        val version = modified?.let { "$expectedSize:$it" }
        val key = creationPreviewCacheKey(uri.normalizeScheme().toString(), safeExtension, version)
        val target = previewCache.materialize(
            directory,
            key,
            safeExtension,
            maximumBytes,
            reusable = version != null,
            openInput = {
                requireNotNull(resolver.openInputStream(uri)) { "Preview is unavailable" }
            },
            validate = { validateCreationPreviewArtifact(it, safeExtension) },
        )
        recentManagedPaths.remember(target)
        pruneDirectory(
            directory,
            MAXIMUM_PREVIEW_FILES,
            PREVIEW_RETENTION_MS,
            MAXIMUM_PREVIEW_CACHE_BYTES,
            setOf(target.absolutePath),
        )
        return target
    }

    private fun pruneDirectory(
        directory: File,
        maximumFiles: Int,
        retentionMs: Long,
        maximumBytes: Long,
        protectedPaths: Set<String>,
    ) {
        if (!creationDurableStateIsReadable(context.filesDir)) {
            throw CreationStorageUnavailableException()
        }
        val now = System.currentTimeMillis()
        val protected = (protectedPaths + durableProtectedPaths())
            .mapNotNull { runCatching { File(it).canonicalPath }.getOrNull() }
            .toSet()
        val grace = recentManagedPaths.protectedPaths()
            .mapNotNull { runCatching { File(it).canonicalPath }.getOrNull() }
            .toSet()
        val candidates = creationRegularFilesNoFollow(directory).sortedBy(File::lastModified)
        var retainedCount = candidates.size
        var retainedBytes = candidates.sumOf { it.length().coerceAtLeast(0L) }
        candidates.forEach { file ->
            val expired = now - file.lastModified() >= retentionMs
            val overCount = retainedCount > maximumFiles
            val overBytes = retainedBytes > maximumBytes
            val hardEviction = overBytes && file.canonicalPath !in protected
            val softEviction = (expired || overCount) &&
                file.canonicalPath !in protected &&
                file.canonicalPath !in grace
            if (hardEviction || softEviction) {
                val length = file.length().coerceAtLeast(0L)
                val isolated = pendingCleanup.isolateAndEnqueue(
                    listOf(CreationCleanupCandidate.trustedManaged(file.absolutePath)),
                )
                if (file.absolutePath in isolated) {
                    retainedCount -= 1
                    retainedBytes -= length
                }
            }
        }
        pendingCleanup.drain()
    }

    private fun durableProtectedPaths(): Set<String> =
        creationDurableProtectedPaths(context.filesDir) +
            synchronized(sourceHandleLock) { sourceHandles.all() }

    private fun managedRoots(): List<File> =
        creationManagedArtifactRoots(context.filesDir, context.cacheDir).map { it.canonicalFile }

    private fun managedRoot(file: File): File? {
        val path = file.toPath().toAbsolutePath().normalize()
        return managedRoots().firstOrNull { path.startsWith(it.toPath()) }
    }

    private fun displayName(uri: Uri): String? = query(uri, OpenableColumns.DISPLAY_NAME)

    private fun query(uri: Uri, column: String): String? {
        var cursor: Cursor? = null
        return try {
            cursor = resolver.query(uri, arrayOf(column), null, null, null)
            if (cursor?.moveToFirst() == true) cursor.getString(0) else null
        } catch (_: Throwable) {
            null
        } finally {
            cursor?.close()
        }
    }

    private companion object {
        const val MAXIMUM_SOURCE_FILES = 256
        const val MAXIMUM_STAGING_FILES = 64
        const val MAXIMUM_PREVIEW_FILES = 64
        const val MAXIMUM_SOURCE_BYTES = CreationContract.MAXIMUM_PICKER_AGGREGATE_BYTES
        const val MAXIMUM_STAGING_BYTES = 512L * 1024 * 1024
        const val MAXIMUM_PREVIEW_CACHE_BYTES = 256L * 1024 * 1024
        const val SOURCE_RETENTION_MS = 30L * 24 * 60 * 60 * 1_000
        const val STAGING_RETENTION_MS = 7L * 24 * 60 * 60 * 1_000
        const val PREVIEW_RETENTION_MS = 24L * 60 * 60 * 1_000
    }
}

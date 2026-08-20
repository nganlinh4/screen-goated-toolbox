package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.provider.OpenableColumns
import androidx.core.content.FileProvider
import java.io.File
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.StandardCopyOption
import java.nio.file.attribute.BasicFileAttributes
import java.util.UUID

internal class CreationOutputStore(
    private val context: Context,
    private val rememberManaged: (File) -> Unit,
) {
    private val resolver = context.contentResolver
    private val safTrees = CreationSafTree(resolver)
    private val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    fun rememberDirectory(uri: Uri): String {
        safTrees.validateAtomicDestination(uri)
        val capability = treeCapability(uri)
        val validated = preferences.getStringSet(KEY_VALIDATED_TREES, emptySet())
            .orEmpty()
            .toMutableSet()
        require(capability in validated || validated.size < MAXIMUM_VALIDATED_TREES) {
            "Too many creation folders are retained"
        }
        validated += capability
        check(
            preferences.edit()
                .putString(KEY_OUTPUT_TREE, uri.toString())
                .putStringSet(KEY_VALIDATED_TREES, validated)
                .commit(),
        ) { "Could not remember creation folder" }
        return directoryLabel(uri)
    }

    fun defaultDirectoryLabel(): String = outputTree()?.let(::directoryLabel) ?: DEFAULT_LABEL

    fun destinationSnapshot(): String? = outputTree()?.toString()

    fun availableBytes(destination: String): Long? = runCatching {
        val tree = Uri.parse(destination)
        val authority = requireNotNull(tree.authority)
        val rootId = DocumentsContract.getTreeDocumentId(tree).substringBefore(':')
        resolver.query(
            DocumentsContract.buildRootsUri(authority),
            arrayOf(
                DocumentsContract.Root.COLUMN_ROOT_ID,
                DocumentsContract.Root.COLUMN_AVAILABLE_BYTES,
            ),
            null,
            null,
            null,
        )?.use { cursor ->
            val idColumn = cursor.getColumnIndex(DocumentsContract.Root.COLUMN_ROOT_ID)
            val bytesColumn = cursor.getColumnIndex(
                DocumentsContract.Root.COLUMN_AVAILABLE_BYTES,
            )
            if (idColumn < 0 || bytesColumn < 0) return@use null
            while (cursor.moveToNext()) {
                if (cursor.getString(idColumn) == rootId && !cursor.isNull(bytesColumn)) {
                    return@use cursor.getLong(bytesColumn).coerceAtLeast(0L)
                }
            }
            null
        }
    }.getOrNull()

    fun plan(
        dispatchId: String,
        requestedName: String,
        mimeType: String,
        destination: String?,
        existingIntents: List<CreationPublishIntent>,
    ): CreationPublishIntent {
        val requested = safeCreationOutputName(requestedName)
        val reserved = existingIntents.map(CreationPublishIntent::finalName).toSet()
        return if (destination == null) {
            val reservationToken = UUID.randomUUID().toString().replace("-", "")
            val directory = File(context.filesDir, "creation/library").apply(File::mkdirs)
            val names = directory.listFiles().orEmpty().map(File::getName).toMutableSet()
                .apply { addAll(reserved) }
            val finalName = uniqueCreationDeliveryName(requested, names, dispatchId)
            CreationPublishIntent(
                kind = MANAGED_KIND,
                finalName = finalName,
                mimeType = mimeType,
                targetPath = File(directory, finalName).absolutePath,
                pendingPath = File(directory, ".sgt-$reservationToken.delivery").absolutePath,
                reservationToken = reservationToken,
            )
        } else {
            val reservationToken = UUID.randomUUID().toString().replace("-", "")
            val tree = Uri.parse(destination)
            val names = safTrees.names(tree) + reserved
            val pendingNames = safTrees.names(tree) +
                existingIntents.mapNotNull(CreationPublishIntent::pendingName)
            val pendingName = uniqueCreationDeliveryName(
                ".sgt-$reservationToken.pending",
                pendingNames.toSet(),
                dispatchId,
            )
            CreationPublishIntent(
                kind = SAF_KIND,
                destination = destination,
                finalName = uniqueCreationDeliveryName(requested, names, dispatchId),
                mimeType = mimeType,
                pendingName = pendingName,
                reservationToken = reservationToken,
            )
        }
    }

    fun reserve(
        intent: CreationPublishIntent,
    ): CreationPendingReservation {
        return when (intent.kind) {
            MANAGED_KIND -> reserveManaged(intent)
            SAF_KIND -> reserveSaf(intent)
            else -> error("Unsupported creation destination")
        }
    }

    fun populate(
        intent: CreationPublishIntent,
        pendingHandle: String,
        pendingIdentity: String,
        source: File,
        expectedSize: Long,
        expectedSha256: String,
    ) {
        require(creationFileMatchesProof(source, expectedSize, expectedSha256)) {
            "Creation result changed before delivery"
        }
        require(artifactIdentity(pendingHandle) == pendingIdentity) {
            "Creation pending output identity changed"
        }
        when (intent.kind) {
            MANAGED_KIND -> populateManaged(intent, pendingHandle, source)
            SAF_KIND -> populateSaf(intent, pendingHandle, source)
            else -> error("Unsupported creation destination")
        }
        require(verifiedPath(pendingHandle, expectedSize, expectedSha256)) {
            "Creation output verification failed"
        }
        require(artifactIdentity(pendingHandle) == pendingIdentity)
    }

    fun commit(
        intent: CreationPublishIntent,
        pendingHandle: String,
        pendingIdentity: String,
        expectedSize: Long,
        expectedSha256: String,
    ): String = when (intent.kind) {
        MANAGED_KIND -> commitManaged(
            intent, pendingHandle, pendingIdentity, expectedSize, expectedSha256,
        )
        SAF_KIND -> commitSaf(
            intent, pendingHandle, pendingIdentity, expectedSize, expectedSha256,
        )
        else -> error("Unsupported creation destination")
    }

    fun recoveredPublication(
        intent: CreationPublishIntent,
        pendingHandle: String,
        pendingIdentity: String,
        expectedSize: Long,
        expectedSha256: String,
    ): String? {
        val final = publishedHandle(intent) ?: return null
        return final.takeIf {
            artifactIdentity(it) == pendingIdentity &&
                verifiedPath(it, expectedSize, expectedSha256)
        }
    }

    fun abort(
        intent: CreationPublishIntent,
        pendingHandle: String,
        pendingIdentity: String,
    ): Boolean = when (intent.kind) {
        MANAGED_KIND -> {
            val pending = File(pendingHandle)
            pending.absolutePath == intent.pendingPath &&
                (!pending.exists() || artifactIdentity(pendingHandle) == pendingIdentity) &&
                (!pending.exists() || pending.delete())
        }
        SAF_KIND -> {
            val pending = Uri.parse(pendingHandle)
            (!pathExists(pendingHandle) ||
                (artifactIdentity(pendingHandle) == pendingIdentity &&
                    query(pending, OpenableColumns.DISPLAY_NAME) == intent.pendingName)) &&
                (runCatching { DocumentsContract.deleteDocument(resolver, pending) }
                    .getOrDefault(false) || !pathExists(pendingHandle))
        }
        else -> false
    }

    fun abortPrepared(
        intent: CreationPublishIntent,
        pendingHandle: String,
        pendingIdentity: String,
        expectedSize: Long,
        expectedSha256: String,
    ): Boolean {
        if (pathExists(pendingHandle) &&
            artifactIdentity(pendingHandle) == pendingIdentity
        ) {
            val name = query(Uri.parse(pendingHandle), OpenableColumns.DISPLAY_NAME)
            if (intent.kind == MANAGED_KIND || name == intent.pendingName) {
                return abort(intent, pendingHandle, pendingIdentity)
            }
            if (name == intent.finalName &&
                verifiedPath(pendingHandle, expectedSize, expectedSha256)
            ) {
                return deleteExactPath(pendingHandle)
            }
        }
        val published = publishedHandle(intent) ?: return true
        if (artifactIdentity(published) != pendingIdentity ||
            !verifiedPath(published, expectedSize, expectedSha256)
        ) return false
        return deleteExactPath(published)
    }

    fun publishedArtifactMatches(
        path: String,
        identity: String,
        expectedSize: Long,
        expectedSha256: String,
    ): Boolean = artifactIdentity(path) == identity &&
        verifiedPath(path, expectedSize, expectedSha256)

    fun identity(path: String): String? = artifactIdentity(path)

    fun planHistoryRenameNames(
        path: String,
        companionPath: String?,
        companionName: String?,
        requestedName: String,
        transactionId: String,
    ): Pair<String, String?> = creationHistoryRenameNames(
        requestedName = safeCreationOutputName(requestedName),
        companionName = companionName,
        primaryOccupied = historyOccupiedNames(path) - File(path).name,
        companionOccupied = companionPath?.let {
            historyOccupiedNames(it) - requireNotNull(companionName)
        }.orEmpty(),
        transactionId = transactionId,
    )

    private fun historyOccupiedNames(path: String): Set<String> =
        path.creationContentUri()?.let { safTrees.names(treeForDocument(it)) }
            ?: File(path).parentFile?.listFiles().orEmpty().map(File::getName).toSet()

    fun renameForHistory(
        path: String,
        targetName: String,
        expectedIdentity: String,
        expectedSize: Long,
        expectedSha256: String,
    ): Pair<String, String> {
        val uri = path.creationContentUri()
        if (uri == null) {
            val source = File(path)
            val directory = requireNotNull(source.parentFile)
            val target = File(directory, targetName)
            if (target.exists()) {
                require(!source.exists())
                require(artifactIdentity(target.absolutePath) == expectedIdentity)
                require(creationFileMatchesProof(target, expectedSize, expectedSha256))
                return target.absolutePath to target.name
            }
            synchronized(managedDeliveryLock) {
                require(artifactIdentity(source.absolutePath) == expectedIdentity)
                require(creationFileMatchesProof(source, expectedSize, expectedSha256))
                require(!target.exists())
                Files.move(source.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE)
            }
            require(artifactIdentity(target.absolutePath) == expectedIdentity)
            require(creationFileMatchesProof(target, expectedSize, expectedSha256))
            forceCreationDirectory(directory)
            return target.absolutePath to target.name
        }
        val tree = treeForDocument(uri)
        require(
            treeCapability(tree) in
                preferences.getStringSet(KEY_VALIDATED_TREES, emptySet()).orEmpty(),
        ) { "This saved folder must be selected again before renaming" }
        safTrees.find(tree, targetName)?.let { existing ->
            require(
                creationSafRenameRecoveryMatches(
                    expectedIdentity,
                    oldHandleExists = pathExists(path),
                    oldHandleIdentity = artifactIdentity(path),
                    targetIdentity = artifactIdentity(existing.toString()),
                ),
            )
            require(verifiedUri(existing, expectedSize, expectedSha256))
            return existing.toString() to targetName
        }
        require(artifactIdentity(path) == expectedIdentity)
        require(verifiedUri(uri, expectedSize, expectedSha256))
        val published = requireNotNull(
            DocumentsContract.renameDocument(resolver, uri, targetName),
        )
        require(query(published, OpenableColumns.DISPLAY_NAME) == targetName)
        require(artifactIdentity(published.toString()) == expectedIdentity)
        require(verifiedUri(published, expectedSize, expectedSha256))
        return published.toString() to targetName
    }

    fun delete(path: String): Boolean = path.creationContentUri()?.let { uri ->
        runCatching {
            DocumentsContract.deleteDocument(resolver, uri) ||
                resolver.delete(uri, null, null) > 0
        }.getOrDefault(false)
    } ?: File(path).delete()

    fun openExternally(path: String) {
        val uri = path.creationContentUri() ?: FileProvider.getUriForFile(
            context,
            "${context.packageName}.fileprovider",
            File(path),
        )
        context.startActivity(
            Intent(Intent.ACTION_VIEW).setData(uri)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }

    fun uploadUri(path: String): Uri = path.creationContentUri() ?: FileProvider.getUriForFile(
        context,
        "${context.packageName}.fileprovider",
        File(path),
    )

    private fun outputTree(): Uri? = preferences.getString(KEY_OUTPUT_TREE, null)
        ?.let(Uri::parse)
        ?.takeIf { uri ->
            resolver.persistedUriPermissions.any { it.uri == uri && it.isWritePermission }
        }

    private fun treeForDocument(uri: Uri): Uri =
        DocumentsContract.buildTreeDocumentUri(
            requireNotNull(uri.authority),
            DocumentsContract.getTreeDocumentId(uri),
        )

    private fun treeCapability(uri: Uri): String =
        "${requireNotNull(uri.authority)}:${DocumentsContract.getTreeDocumentId(uri)}"

    private fun reserveManaged(intent: CreationPublishIntent): CreationPendingReservation {
        require(publishedHandle(intent) == null) { "Creation destination is already occupied" }
        val pending = File(requireNotNull(intent.pendingPath))
        require(
            pending.name == ".sgt-${requireNotNull(intent.reservationToken)}.delivery",
        )
        require(pending.parentFile?.canonicalFile ==
            File(context.filesDir, "creation/library").canonicalFile)
        if (!pending.exists()) check(pending.createNewFile()) { "Could not reserve output file" }
        return CreationPendingReservation(
            pending.absolutePath,
            requireNotNull(artifactIdentity(pending.absolutePath)),
        )
    }

    private fun populateManaged(
        intent: CreationPublishIntent,
        pendingHandle: String,
        source: File,
    ) {
        val pending = File(requireNotNull(intent.pendingPath))
        require(pending.absolutePath == pendingHandle)
        writeCreationFileDurably(source, pending)
    }

    private fun commitManaged(
        intent: CreationPublishIntent,
        pendingHandle: String,
        pendingIdentity: String,
        expectedSize: Long,
        expectedSha256: String,
    ): String {
        val target = File(requireNotNull(intent.targetPath))
        val pending = File(pendingHandle)
        if (target.exists()) {
            require(
                artifactIdentity(target.absolutePath) == pendingIdentity &&
                    verifiedFile(target, expectedSize, expectedSha256),
            ) {
                "Creation destination is already occupied"
            }
            return target.absolutePath
        }
        require(verifiedFile(pending, expectedSize, expectedSha256))
        require(artifactIdentity(pending.absolutePath) == pendingIdentity)
        val root = File(context.filesDir, "creation/library")
        synchronized(managedDeliveryLock) {
            if (!target.exists()) {
                Files.move(pending.toPath(), target.toPath())
                forceCreationDirectory(root)
            }
        }
        require(verifiedFile(target, expectedSize, expectedSha256))
        require(artifactIdentity(target.absolutePath) == pendingIdentity)
        rememberManaged(target)
        return target.absolutePath
    }

    private fun reserveSaf(intent: CreationPublishIntent): CreationPendingReservation {
        val tree = Uri.parse(requireNotNull(intent.destination))
        require(safTrees.find(tree, intent.finalName) == null) {
            "Creation destination is already occupied"
        }
        val pendingName = requireNotNull(intent.pendingName)
        require(
            pendingName == ".sgt-${requireNotNull(intent.reservationToken)}.pending",
        )
        safTrees.find(tree, pendingName)?.let { existing ->
            return CreationPendingReservation(
                existing.toString(),
                requireNotNull(artifactIdentity(existing.toString())),
            )
        }
        val parentId = DocumentsContract.getTreeDocumentId(tree)
        val parent = DocumentsContract.buildDocumentUriUsingTree(tree, parentId)
        val pending = requireNotNull(
            DocumentsContract.createDocument(resolver, parent, intent.mimeType, pendingName),
        ) { "Could not reserve output file" }.toString()
        return CreationPendingReservation(
            pending,
            requireNotNull(artifactIdentity(pending)),
        )
    }

    private fun populateSaf(
        intent: CreationPublishIntent,
        pendingHandle: String,
        source: File,
    ) {
        val pending = Uri.parse(pendingHandle)
        require(query(pending, OpenableColumns.DISPLAY_NAME) == intent.pendingName)
        requireNotNull(resolver.openFileDescriptor(pending, "w")) {
            "Could not write output file"
        }.use { descriptor ->
            FileOutputStream(descriptor.fileDescriptor).use { output ->
                source.inputStream().use { it.copyTo(output) }
                output.fd.sync()
            }
        }
    }

    private fun commitSaf(
        intent: CreationPublishIntent,
        pendingHandle: String,
        pendingIdentity: String,
        expectedSize: Long,
        expectedSha256: String,
    ): String {
        val tree = Uri.parse(requireNotNull(intent.destination))
        val pending = Uri.parse(pendingHandle)
        safTrees.find(tree, intent.finalName)?.let { published ->
            require(
                artifactIdentity(published.toString()) == pendingIdentity &&
                    verifiedUri(published, expectedSize, expectedSha256),
            ) { "Creation destination is already occupied" }
            return published.toString()
        }
        require(verifiedUri(pending, expectedSize, expectedSha256)) {
            "Creation output verification failed"
        }
        require(artifactIdentity(pendingHandle) == pendingIdentity)
        require(safTrees.find(tree, intent.finalName) == null) {
            "Creation destination is already occupied"
        }
        val published = requireNotNull(
            DocumentsContract.renameDocument(resolver, pending, intent.finalName),
        ) { "The selected folder cannot publish files atomically" }
        require(verifiedUri(published, expectedSize, expectedSha256)) {
            "Creation output verification failed"
        }
        require(query(published, OpenableColumns.DISPLAY_NAME) == intent.finalName) {
            "The selected folder changed the output name"
        }
        require(artifactIdentity(published.toString()) == pendingIdentity)
        return published.toString()
    }

    private fun verifiedUri(uri: Uri, size: Long, digest: String): Boolean =
        runCatching {
            val observedSize = query(uri, OpenableColumns.SIZE)?.toLongOrNull()
            observedSize == size && resolver.openInputStream(uri)?.use {
                creationStreamSha256(it)
            }?.equals(digest, ignoreCase = true) == true
        }.getOrDefault(false)

    private fun verifiedFile(file: File, size: Long, digest: String): Boolean =
        file.isFile && file.length() == size &&
            runCatching { creationFileSha256(file).equals(digest, ignoreCase = true) }
                .getOrDefault(false)

    private fun publishedHandle(intent: CreationPublishIntent): String? = when (intent.kind) {
        MANAGED_KIND -> intent.targetPath?.takeIf { File(it).exists() }
        SAF_KIND -> safTrees.find(
            Uri.parse(requireNotNull(intent.destination)),
            intent.finalName,
        )?.toString()
        else -> null
    }

    private fun pathExists(path: String): Boolean =
        path.creationContentUri()?.let { uri ->
            runCatching { resolver.openAssetFileDescriptor(uri, "r")?.use { true } ?: false }
                .getOrDefault(false)
        } ?: File(path).exists()

    private fun verifiedPath(path: String, size: Long, digest: String): Boolean =
        path.creationContentUri()?.let { verifiedUri(it, size, digest) }
            ?: verifiedFile(File(path), size, digest)

    private fun artifactIdentity(path: String): String? =
        path.creationContentUri()?.let { uri ->
            runCatching {
                "${requireNotNull(uri.authority)}:${DocumentsContract.getDocumentId(uri)}"
            }.getOrNull()
        } ?: runCatching {
            val file = File(path)
            val attributes = Files.readAttributes(
                file.toPath(),
                BasicFileAttributes::class.java,
                LinkOption.NOFOLLOW_LINKS,
            )
            attributes.fileKey()?.toString()
        }.getOrNull()

    private fun deleteExactPath(path: String): Boolean =
        path.creationContentUri()?.let { uri ->
            runCatching { DocumentsContract.deleteDocument(resolver, uri) }
                .getOrDefault(false) || !pathExists(path)
        } ?: File(path).delete()

    private fun directoryLabel(uri: Uri): String {
        val name = query(uri, DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            ?: query(
                DocumentsContract.buildDocumentUriUsingTree(
                    uri,
                    DocumentsContract.getTreeDocumentId(uri),
                ),
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            )
        return name?.let { "Storage/$it" } ?: "Storage"
    }

    private fun query(uri: Uri, column: String): String? = runCatching {
        resolver.query(uri, arrayOf(column), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) cursor.getString(0) else null
        }
    }.getOrNull()

    private companion object {
        const val PREFERENCES = "creation_output"
        const val KEY_OUTPUT_TREE = "tree_uri"
        const val KEY_VALIDATED_TREES = "validated_trees"
        const val MAXIMUM_VALIDATED_TREES = 4_096
        const val DEFAULT_LABEL = "SGT Library"
        const val MANAGED_KIND = "managed"
        const val SAF_KIND = "saf"
    }
}

internal fun creationSafRenameRecoveryMatches(
    expectedIdentity: String,
    oldHandleExists: Boolean,
    oldHandleIdentity: String?,
    targetIdentity: String?,
): Boolean = targetIdentity == expectedIdentity &&
    (!oldHandleExists || oldHandleIdentity == expectedIdentity)

private val managedDeliveryLock = Any()

private fun String.creationContentUri(): Uri? =
    takeIf { startsWith("content://") }?.let(Uri::parse)

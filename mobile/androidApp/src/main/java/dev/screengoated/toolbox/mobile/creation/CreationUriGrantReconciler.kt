package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener

@Serializable
internal data class CreationOwnedUriGrant(
    val uri: String,
    val roles: Set<String>,
    val ownedFlags: Int,
    val pendingFlags: Int = 0,
)

internal class CreationUriGrantLedger(private val context: Context) {
    private val target = File(context.filesDir, "creation/state/uri-grants.json")
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }
    private val lock = creationUriGrantLedgerLock

    fun persistSource(uri: Uri): Boolean =
        persist(uri, SOURCE_KIND, Intent.FLAG_GRANT_READ_URI_PERMISSION)

    fun persistOutput(uri: Uri): Boolean = persist(
        uri,
        OUTPUT_KIND,
        Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
    )

    fun owned(): List<CreationOwnedUriGrant> = synchronized(lock) { read() }

    fun reconcile(grant: CreationOwnedUriGrant, requiredRoles: Set<String>) {
        synchronized(lock) {
            val resolver = context.contentResolver
            val uri = Uri.parse(grant.uri)
            val neededFlags = requiredRoles.fold(0) { flags, role ->
                flags or flagsForCreationGrantRole(role)
            }
            val pendingNeeded = grant.pendingFlags and neededFlags
            val before = persistedCreationGrantFlags(resolver, uri)
            val missingPending = pendingNeeded and before.inv()
            if (missingPending != 0) {
                runCatching { resolver.takePersistableUriPermission(uri, missingPending) }
                val after = persistedCreationGrantFlags(resolver, uri)
                if (after and missingPending != missingPending) return
            }
            val acquiredPending = pendingNeeded
            val releaseFlags = (grant.ownedFlags or grant.pendingFlags) and neededFlags.inv()
            val released = releaseFlags == 0 || runCatching {
                val available = persistedCreationGrantFlags(resolver, uri)
                val held = releaseFlags and available
                if (held != 0) resolver.releasePersistableUriPermission(uri, held)
                true
            }.getOrDefault(false)
            if (!released) return
            val records = read().filterNot { it.uri == grant.uri }.toMutableList()
            val retainedFlags = (grant.ownedFlags and neededFlags) or acquiredPending
            if (requiredRoles.isNotEmpty() && retainedFlags != 0) {
                records += grant.copy(
                    roles = requiredRoles,
                    ownedFlags = retainedFlags,
                    pendingFlags = 0,
                )
            }
            write(records)
        }
    }

    private fun persist(uri: Uri, kind: String, flags: Int): Boolean = synchronized(lock) {
        val resolver = context.contentResolver
        val beforeFlags = persistedCreationGrantFlags(resolver, uri)
        val records = read().toMutableList()
        val previous = records.firstOrNull { it.uri == uri.toString() }
        val newlyOwned = flags and beforeFlags.inv()
        val planned = CreationOwnedUriGrant(
            uri.toString(),
            previous?.roles.orEmpty() + kind,
            previous?.ownedFlags ?: 0,
            (previous?.pendingFlags ?: 0) or newlyOwned,
        )
        records.removeAll { it.uri == uri.toString() }
        if (planned.ownedFlags != 0 || planned.pendingFlags != 0) records += planned
        write(records)
        runCatching { resolver.takePersistableUriPermission(uri, flags) }
        val available = resolver.persistedUriPermissions.any { permission ->
            permission.uri == uri &&
                (flags and Intent.FLAG_GRANT_READ_URI_PERMISSION == 0 ||
                    permission.isReadPermission) &&
                (flags and Intent.FLAG_GRANT_WRITE_URI_PERMISSION == 0 ||
                    permission.isWritePermission)
        }
        if (!available) {
            val acquired = persistedCreationGrantFlags(resolver, uri) and newlyOwned
            if (acquired != 0) {
                runCatching { resolver.releasePersistableUriPermission(uri, acquired) }
            }
            val rollback = read().filterNot { it.uri == uri.toString() }.toMutableList()
            previous?.let(rollback::add)
            write(rollback)
        } else {
            val afterFlags = persistedCreationGrantFlags(resolver, uri)
            val committable = planned.pendingFlags and afterFlags
            val committed = read().filterNot { it.uri == uri.toString() }.toMutableList()
            val updated = planned.copy(
                ownedFlags = planned.ownedFlags or committable,
                pendingFlags = planned.pendingFlags and committable.inv(),
            )
            if (updated.ownedFlags != 0 || updated.pendingFlags != 0) committed += updated
            write(committed)
        }
        available
    }

    private fun read(): List<CreationOwnedUriGrant> {
        if (!target.exists()) return emptyList()
        val text = requireNotNull(
            readCreationIndexTextBounded(target, CREATION_URI_GRANT_INDEX_MAX_BYTES),
        )
        val records = json.decodeFromString<List<CreationOwnedUriGrant>>(text)
        require(
            records.size <= CREATION_URI_GRANT_MAXIMUM_RECORDS &&
                records.map(CreationOwnedUriGrant::uri).distinct().size == records.size &&
                records.all(::validCreationOwnedUriGrant),
        )
        return records
    }

    private fun write(records: List<CreationOwnedUriGrant>) {
        require(records.size <= CREATION_URI_GRANT_MAXIMUM_RECORDS)
        writeCreationIndexTextAtomically(
            target,
            json.encodeToString(records),
            CREATION_URI_GRANT_INDEX_MAX_BYTES,
        )
    }

    companion object {
        const val SOURCE_KIND = "source"
        const val OUTPUT_KIND = "output"
    }
}

internal fun CreationFileStore.reconcilePersistedUriGrants() {
    val required = creationRequiredPersistedUriGrants(
        context.filesDir,
        leasedSourceHandles(),
        outputDestinationSnapshot(),
    )
    uriGrants.owned().forEach { grant ->
        uriGrants.reconcile(grant, creationRequiredGrantRoles(grant.uri, required))
    }
}

internal data class CreationRequiredUriGrants(
    val source: Set<String>,
    val output: Set<String>,
)

internal fun creationRequiredGrantRoles(
    uri: String,
    required: CreationRequiredUriGrants,
): Set<String> = buildSet {
    if (required.source.any { creationUriGrantProtects(uri, it) }) {
        add(CreationUriGrantLedger.SOURCE_KIND)
    }
    if (required.output.any { creationUriGrantProtects(uri, it) }) {
        add(CreationUriGrantLedger.OUTPUT_KIND)
    }
}

internal fun creationRequiredPersistedUriGrants(
    filesDir: File,
    leasedSources: Set<String>,
    currentOutputTree: String? = null,
): CreationRequiredUriGrants {
    val source = leasedSources.filterTo(mutableSetOf(), String::isCreationContentHandle)
    val output = mutableSetOf<String>().apply { currentOutputTree?.let(::add) }
    collectCreationGrantKeys(
        File(filesDir, "creation/state/accepted-jobs.json"),
        CREATION_JOURNAL_INDEX_MAX_BYTES,
        source,
        output,
    )
    collectCreationGrantKeys(
        File(filesDir, "creation/state/deliveries.json"),
        CREATION_DELIVERY_INDEX_MAX_BYTES,
        source,
        output,
    )
    collectCreationGrantKeys(
        File(filesDir, "creation/history.json"),
        CREATION_HISTORY_INDEX_MAX_BYTES,
        mutableSetOf(),
        output,
    )
    collectCreationGrantKeys(
        File(filesDir, "creation/state/history-renames.json"),
        CREATION_RENAME_INDEX_MAX_BYTES,
        mutableSetOf(),
        output,
    )
    return CreationRequiredUriGrants(source, output)
}

internal fun creationUriGrantProtects(permission: String, required: String): Boolean {
    if (permission == required) return true
    return runCatching {
        val grant = Uri.parse(permission)
        val handle = Uri.parse(required)
        grant.authority == handle.authority &&
            DocumentsContract.isTreeUri(grant) &&
            DocumentsContract.getTreeDocumentId(grant) ==
            DocumentsContract.getTreeDocumentId(handle)
    }.getOrDefault(false)
}

private fun collectCreationGrantKeys(
    file: File,
    maximumBytes: Long,
    source: MutableSet<String>,
    output: MutableSet<String>,
) {
    readCreationIndexTextBounded(file, maximumBytes)
        ?.let { runCatching { JSONTokener(it).nextValue() }.getOrNull() }
        ?.let { collectCreationGrantKeys(it, null, source, output) }
}

private fun collectCreationGrantKeys(
    value: Any?,
    key: String?,
    source: MutableSet<String>,
    output: MutableSet<String>,
) {
    when (value) {
        is JSONObject -> value.keys().forEach {
            collectCreationGrantKeys(value.opt(it), it, source, output)
        }
        is JSONArray -> repeat(value.length()) {
            collectCreationGrantKeys(value.opt(it), key, source, output)
        }
        is String -> when {
            !value.isCreationContentHandle() -> Unit
            key in CREATION_SOURCE_GRANT_KEYS -> source += value
            key in CREATION_OUTPUT_GRANT_KEYS -> output += value
        }
    }
}

private fun String.isCreationContentHandle(): Boolean = startsWith("content://")

private val CREATION_SOURCE_GRANT_KEYS = setOf(
    "sourceImagePath",
    "sourceImagePaths",
)
private val CREATION_OUTPUT_GRANT_KEYS = setOf(
    "destination",
    "outputPath",
    "publishedPath",
    "oldPath",
    "newPath",
)
internal const val CREATION_URI_GRANT_INDEX_MAX_BYTES = 2L * 1024 * 1024
private const val CREATION_URI_GRANT_MAXIMUM_RECORDS = 8_192
private val creationUriGrantLedgerLock = Any()

private fun validCreationOwnedUriGrant(grant: CreationOwnedUriGrant): Boolean =
    grant.uri.startsWith("content://") &&
        grant.roles.isNotEmpty() &&
        grant.roles.all { it in setOf(
            CreationUriGrantLedger.SOURCE_KIND,
            CreationUriGrantLedger.OUTPUT_KIND,
        ) } &&
        (grant.ownedFlags != 0 || grant.pendingFlags != 0) &&
        (grant.ownedFlags or grant.pendingFlags) and (
            Intent.FLAG_GRANT_READ_URI_PERMISSION or
                Intent.FLAG_GRANT_WRITE_URI_PERMISSION
            ) == (grant.ownedFlags or grant.pendingFlags)

private fun flagsForCreationGrantRole(role: String): Int = when (role) {
    CreationUriGrantLedger.SOURCE_KIND -> Intent.FLAG_GRANT_READ_URI_PERMISSION
    CreationUriGrantLedger.OUTPUT_KIND ->
        Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
    else -> 0
}

private fun persistedCreationGrantFlags(
    resolver: android.content.ContentResolver,
    uri: Uri,
): Int = resolver.persistedUriPermissions.firstOrNull { it.uri == uri }?.let {
    (if (it.isReadPermission) Intent.FLAG_GRANT_READ_URI_PERMISSION else 0) or
        (if (it.isWritePermission) Intent.FLAG_GRANT_WRITE_URI_PERMISSION else 0)
} ?: 0

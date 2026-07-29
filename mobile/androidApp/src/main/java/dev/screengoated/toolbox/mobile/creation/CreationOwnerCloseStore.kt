package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

@Serializable
internal data class CreationOwnerCloseRecord(
    val ownerId: String,
    val tool: String,
    val createdAtMs: Long,
    val requests: List<CreationWorkerRequest> = emptyList(),
    val busyJobIds: Set<String> = emptySet(),
    val stateRetired: Boolean = false,
)

internal class CreationOwnerCloseStore(context: Context) {
    private val target = File(context.filesDir, "creation/state/owner-closes.json")
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }
    private val lock = Any()

    fun begin(ownerId: String, tool: CreationTool) = synchronized(lock) {
        require(ownerId.length in 1..256)
        val records = read().toMutableList()
        if (records.any { it.ownerId == ownerId && it.tool == tool.wireName }) return@synchronized
        require(records.size < CREATION_OWNER_CLOSE_MAXIMUM_RECORDS) {
            CREATION_STORAGE_UNAVAILABLE_ERROR_KEY
        }
        records += CreationOwnerCloseRecord(ownerId, tool.wireName, System.currentTimeMillis())
        write(records)
    }

    fun pending(): List<CreationOwnerCloseRecord> = synchronized(lock) { read() }

    fun prepare(
        ownerId: String,
        tool: CreationTool,
        requests: List<CreationWorkerRequest>,
        busyJobIds: Set<String>,
    ): CreationOwnerCloseRecord = synchronized(lock) {
        require(busyJobIds.all { id -> requests.any { it.jobId == id } })
        val records = read().toMutableList()
        val index = records.indexOfFirst {
            it.ownerId == ownerId && it.tool == tool.wireName
        }
        val current = requireNotNull(records.getOrNull(index))
        if (current.requests.isNotEmpty() || current.stateRetired) return@synchronized current
        val prepared = current.copy(requests = requests, busyJobIds = busyJobIds)
        records[index] = prepared
        write(records)
        prepared
    }

    fun markStateRetired(ownerId: String, tool: CreationTool): CreationOwnerCloseRecord =
        synchronized(lock) {
            val records = read().toMutableList()
            val index = records.indexOfFirst {
                it.ownerId == ownerId && it.tool == tool.wireName
            }
            val current = requireNotNull(records.getOrNull(index))
            val retired = current.copy(stateRetired = true)
            records[index] = retired
            write(records)
            retired
        }

    fun complete(ownerId: String, tool: CreationTool) = synchronized(lock) {
        val records = read().filterNot {
            it.ownerId == ownerId && it.tool == tool.wireName
        }
        write(records)
    }

    private fun read(): List<CreationOwnerCloseRecord> {
        if (!target.exists()) return emptyList()
        val text = requireNotNull(
            readCreationIndexTextBounded(target, CREATION_OWNER_CLOSE_INDEX_MAX_BYTES),
        ) { CREATION_STORAGE_UNAVAILABLE_ERROR_KEY }
        val decoded = json.decodeFromString<List<CreationOwnerCloseRecord>>(text)
        require(
            decoded.size <= CREATION_OWNER_CLOSE_MAXIMUM_RECORDS &&
                decoded.distinctBy { it.ownerId to it.tool }.size == decoded.size &&
                decoded.all(::validCreationOwnerCloseRecord),
        ) { CREATION_STORAGE_UNAVAILABLE_ERROR_KEY }
        return decoded
    }

    private fun write(records: List<CreationOwnerCloseRecord>) {
        require(
            records.size <= CREATION_OWNER_CLOSE_MAXIMUM_RECORDS &&
                records.distinctBy { it.ownerId to it.tool }.size == records.size &&
                records.all(::validCreationOwnerCloseRecord),
        ) { CREATION_STORAGE_UNAVAILABLE_ERROR_KEY }
        writeCreationIndexTextAtomically(
            target,
            json.encodeToString(records),
            CREATION_OWNER_CLOSE_INDEX_MAX_BYTES,
        )
    }
}

internal const val CREATION_OWNER_CLOSE_INDEX_MAX_BYTES = 4L * 1024 * 1024
private const val CREATION_OWNER_CLOSE_MAXIMUM_RECORDS = 1_024

private fun validCreationOwnerCloseRecord(record: CreationOwnerCloseRecord): Boolean =
    record.ownerId.length in 1..256 &&
        CreationTool.fromWireName(record.tool) != null &&
        record.createdAtMs >= 0L &&
        record.requests.size <= CREATION_JOURNAL_MAXIMUM_RECORDS &&
        record.requests.distinctBy(CreationWorkerRequest::jobId).size == record.requests.size &&
        record.requests.all {
            it.tool == record.tool && creationRequestHasValidDeliveryIdentity(it)
        } &&
        record.busyJobIds.all { id -> record.requests.any { it.jobId == id } }

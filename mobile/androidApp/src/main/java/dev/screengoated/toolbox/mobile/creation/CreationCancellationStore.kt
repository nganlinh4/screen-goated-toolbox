package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.json.JSONArray

@Serializable
internal data class CreationCancellationFence(
    val jobId: String,
    val dispatchId: String,
    val requestFingerprint: String,
    val createdAtMs: Long,
)

internal class CreationCancellationStore(context: Context) {
    private val target = File(context.filesDir, "creation/state/cancellations.json")
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }

    fun record(requests: Collection<CreationWorkerRequest>) {
        if (requests.isEmpty()) return
        synchronized(creationCancellationFenceLock) {
            val now = System.currentTimeMillis()
            val unresolved = unresolvedDispatchIds()
            val retained = retainCreationCancellationFences(read(), now, unresolved)
                .associateBy(CreationCancellationFence::dispatchId).toMutableMap()
            requests.forEach { request ->
                require(creationRequestHasValidDeliveryIdentity(request))
                retained[request.dispatchId] = CreationCancellationFence(
                    request.jobId,
                    request.dispatchId,
                    request.requestFingerprint,
                    now,
                )
            }
            val compacted = compactCreationCancellationFences(
                retained.values,
                unresolved + requests.map(CreationWorkerRequest::dispatchId),
            )
            write(compacted)
        }
    }

    fun reserveAcceptance(dispatchId: String) {
        synchronized(creationCancellationFenceLock) {
            val unresolved = unresolvedDispatchIds() + dispatchId
            require(unresolved.size <= CREATION_CANCELLATION_MAXIMUM_RECORDS) {
                "Creation cancellation state exceeds capacity"
            }
            val compacted = compactCreationCancellationFences(
                retainCreationCancellationFences(
                    read(),
                    System.currentTimeMillis(),
                    unresolved,
                ),
                unresolved,
            )
            write(compacted)
        }
    }

    fun isCancelled(request: CreationWorkerRequest): Boolean =
        synchronized(creationCancellationFenceLock) { matches(read(), request) }

    fun <T> ifActive(request: CreationWorkerRequest, action: () -> T): T =
        synchronized(creationCancellationFenceLock) {
            require(!matches(read(), request)) { "Creation was cancelled" }
            action()
        }

    fun applyTo(records: List<RestoredCreationRecord>): List<RestoredCreationRecord> =
        synchronized(creationCancellationFenceLock) {
            val fences = read()
            records.map { record ->
                if (matches(fences, record.request)) {
                    record.copy(
                        status = record.status.copy(
                            stage = "cancelled",
                            progressText = "Cancelled.",
                            phase = "cancelled",
                        ),
                    )
                } else {
                    record
                }
            }
        }

    private fun matches(
        fences: List<CreationCancellationFence>,
        request: CreationWorkerRequest,
    ): Boolean = fences.any {
        it.jobId == request.jobId &&
            it.dispatchId == request.dispatchId &&
            it.requestFingerprint == request.requestFingerprint
    }

    private fun read(): List<CreationCancellationFence> {
        if (!target.exists()) return emptyList()
        val text = requireNotNull(
            readCreationIndexTextBounded(target, CREATION_CANCELLATION_INDEX_MAX_BYTES),
        ) { "Creation cancellation state is unavailable" }
        val decoded = json.decodeFromString<List<CreationCancellationFence>>(text)
        require(
            decoded.size <= CREATION_CANCELLATION_MAXIMUM_RECORDS &&
                decoded.map(CreationCancellationFence::dispatchId).distinct().size == decoded.size &&
                decoded.all(::validCreationCancellationFence),
        ) { "Creation cancellation state exceeds capacity" }
        return decoded
    }

    private fun write(records: List<CreationCancellationFence>) {
        writeCreationIndexTextAtomically(
            target,
            json.encodeToString(records),
            CREATION_CANCELLATION_INDEX_MAX_BYTES,
        )
    }

    private fun unresolvedDispatchIds(): Set<String> = buildSet {
        listOf(
            File(target.parentFile, "accepted-jobs.json") to CREATION_JOURNAL_INDEX_MAX_BYTES,
            File(target.parentFile, "deliveries.json") to CREATION_DELIVERY_INDEX_MAX_BYTES,
        ).forEach { (file, limit) ->
            if (!file.exists()) return@forEach
            val values = JSONArray(
                requireNotNull(readCreationIndexTextBounded(file, limit)) {
                    "Creation cancellation state is unavailable"
                },
            )
            repeat(values.length()) { index ->
                val record = values.getJSONObject(index)
                val dispatchId = if (record.has("dispatchId")) {
                    record.getString("dispatchId")
                } else {
                    record.getJSONObject("request").getString("dispatchId")
                }
                add(dispatchId)
            }
        }
    }
}

internal fun CreationFileStore.retireCancelledCreationRequest(
    request: CreationWorkerRequest,
): Boolean {
    val output = managedPathIdentity(request.outputPath)
    val outputRetired = if (output != null && exists(output)) {
        output in pendingCleanupStore().isolateAndEnqueue(
            listOf(CreationCleanupCandidate.trustedManaged(output)),
        )
    } else {
        true
    }
    return releaseJobInputs(request.imagePaths) && outputRetired
}

internal const val CREATION_CANCELLATION_INDEX_MAX_BYTES = 4L * 1024 * 1024
private const val CREATION_CANCELLATION_MAXIMUM_RECORDS = 16_384
internal const val CREATION_CANCELLATION_RETENTION_MS = 7L * 24 * 60 * 60 * 1_000
private val creationCancellationFenceLock = Any()

internal fun retainCreationCancellationFences(
    records: List<CreationCancellationFence>,
    nowMs: Long,
    unresolvedDispatchIds: Set<String> = emptySet(),
): List<CreationCancellationFence> = records.filter {
    it.dispatchId in unresolvedDispatchIds ||
        it.createdAtMs in
        (nowMs - CREATION_CANCELLATION_RETENTION_MS)..(nowMs + CREATION_CLOCK_SKEW_MS)
}

internal fun compactCreationCancellationFences(
    records: Collection<CreationCancellationFence>,
    protectedDispatchIds: Set<String>,
): List<CreationCancellationFence> {
    val sorted = records.distinctBy(CreationCancellationFence::dispatchId)
        .sortedBy(CreationCancellationFence::createdAtMs)
        .toMutableList()
    while (sorted.size > CREATION_CANCELLATION_MAXIMUM_RECORDS) {
        val removable = sorted.indexOfFirst { it.dispatchId !in protectedDispatchIds }
        require(removable >= 0) { "Creation cancellation state exceeds capacity" }
        sorted.removeAt(removable)
    }
    return sorted
}

private fun validCreationCancellationFence(record: CreationCancellationFence): Boolean =
    record.jobId.length in 1..256 &&
        record.dispatchId.length in 1..256 &&
        record.requestFingerprint.length == 64 &&
        record.requestFingerprint.all(Char::isHexDigit) &&
        record.createdAtMs >= 0L

private fun Char.isHexDigit(): Boolean =
    this in '0'..'9' || this in 'a'..'f' || this in 'A'..'F'

private const val CREATION_CLOCK_SKEW_MS = 5L * 60 * 1_000

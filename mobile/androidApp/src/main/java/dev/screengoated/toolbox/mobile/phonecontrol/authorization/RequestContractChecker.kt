package dev.screengoated.toolbox.mobile.phonecontrol.authorization

import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlTurnRecorder
import java.security.MessageDigest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.put

/**
 * The one request-contract checkpoint shared by every exact-effect authorizer,
 * ported from Windows `uia_task/request_authorization.rs`: bounded user-authored
 * request history recorded per turn, and the independent model quorum. An
 * authorizer owns only its proposal context, instruction, and result codes.
 * Code never interprets request wording; it compares identities and verdicts.
 */
internal class RequestContractChecker(
    private val modelIds: () -> List<String>,
    private val candidateClient: RequestContractCandidateClient,
) : PhoneControlTurnRecorder {
    private data class RequestRecord(
        val turnId: Long,
        var text: String,
        var completed: Boolean,
        var interrupted: Boolean = false,
    )

    private val lock = Any()
    private val requests = linkedMapOf<Long, RequestRecord>()
    private var revision = 0L
    private var carryInterruptedLineage = false

    /**
     * Windows `request_authorization::begin_turn(turn, inherit)`. A turn that
     * follows one interrupted while still acting continues its goal, as a
     * Windows transcript does when it arrives while the action is active or
     * ended unaccepted; it keeps and re-adopts the earlier requests. Any other
     * turn keeps only its own request.
     */
    override fun turnStarted(turnId: Long, generation: Long) = mutate {
        val current = requests[turnId]
        if (carryInterruptedLineage) {
            requests.values.forEach { it.interrupted = false }
        } else {
            requests.clear()
        }
        carryInterruptedLineage = false
        requests[turnId] = current ?: RequestRecord(turnId, "", completed = false)
        trimHistoryLocked()
    }

    override fun userTranscriptUpdated(turnId: Long, text: String) {
        val normalized = text.trim()
        if (normalized.isEmpty()) return
        mutate {
            requests.getOrPut(turnId) { RequestRecord(turnId, "", completed = false) }.text = normalized
            trimHistoryLocked()
        }
    }

    override fun userRequestSubmitted(turnId: Long, text: String) = userTranscriptUpdated(turnId, text)

    override fun assistantTranscriptUpdated(turnId: Long, text: String) = Unit

    override fun turnCompleted(turnId: Long, userText: String, assistantText: String) = mutate {
        val normalized = userText.trim()
        if (normalized.isNotEmpty()) {
            requests[turnId] = RequestRecord(turnId, normalized, completed = true)
        } else {
            requests.remove(turnId)
        }
        trimHistoryLocked()
    }

    /**
     * An interrupted request alone is never authority. It stays in the lineage
     * for the next turn only, which adopts it again; a turn after that drops it.
     */
    override fun turnInterrupted(turnId: Long) = mutate {
        requests[turnId]?.interrupted = true
        carryInterruptedLineage = true
    }

    /** The committed request history and the revision it was read at, or null when none exists. */
    fun history(): RequestHistorySnapshot? = synchronized(lock) {
        val history = requests.values.filter { it.text.isNotBlank() && !it.interrupted }
        if (history.isEmpty()) return null
        RequestHistorySnapshot(
            requests = buildJsonArray {
                history.forEach { request ->
                    add(buildJsonObject {
                        put("turn_id", request.turnId)
                        put("text", request.text)
                    })
                }
            },
            revision = revision,
        )
    }

    fun isCurrent(revision: Long): Boolean = synchronized(lock) { this.revision == revision }

    /** The bounded independent quorum, or null when it did not finish in time. */
    suspend fun assess(instruction: String, context: String): RequestContractReport? {
        val candidates = runCatching { modelIds() }.getOrDefault(emptyList())
            .map(String::trim)
            .filter(String::isNotEmpty)
            .distinct()
        return withTimeoutOrNull(TOTAL_TIMEOUT_MS) {
            evaluateCandidates(candidates, instruction, context)
        }
    }

    private suspend fun evaluateCandidates(
        candidates: List<String>,
        instruction: String,
        context: String,
    ): RequestContractReport {
        var positive = 0
        var negative = 0
        var malformed = 0
        var failed = 0
        var reason: String? = null
        val attempted = mutableListOf<String>()
        for (modelId in candidates) {
            attempted += modelId
            val response = try {
                withTimeoutOrNull(PROVIDER_TIMEOUT_MS) {
                    candidateClient.request(modelId, instruction, context)
                }
            } catch (error: CancellationException) {
                throw error
            } catch (_: Throwable) {
                failed += 1
                continue
            }
            if (response == null) {
                failed += 1
                continue
            }
            val error = response.exceptionOrNull()
            if (error != null) {
                if (error is CancellationException) throw error
                failed += 1
                continue
            }
            val verdict = response.getOrNull()?.let(::parseRequestContractVerdict)
            if (verdict == null) {
                malformed += 1
                continue
            }
            reason = reason ?: verdict.reason
            if (verdict.authorized) {
                positive += 1
                if (positive >= MIN_POSITIVE_VERDICTS) break
            } else {
                negative += 1
                reason = verdict.reason
                break
            }
        }
        return RequestContractReport(
            positive = positive,
            negative = negative,
            malformed = malformed,
            failed = failed,
            reason = reason ?: "No independent model returned a valid authorization verdict.",
            attemptedModels = attempted,
        )
    }

    private inline fun mutate(change: () -> Unit) {
        synchronized(lock) {
            change()
            revision += 1
        }
    }

    private fun trimHistoryLocked() {
        requests.entries.removeAll { it.value.text.isBlank() && it.value.completed }
        while (
            requests.size > MAX_REQUEST_TURNS ||
            requests.values.sumOf { it.text.length } > MAX_REQUEST_CHARS
        ) {
            requests.remove(requests.keys.firstOrNull() ?: break)
        }
    }

    private companion object {
        const val MAX_REQUEST_TURNS = 6
        const val MAX_REQUEST_CHARS = 12_000
        const val TOTAL_TIMEOUT_MS = 28_000L
        const val PROVIDER_TIMEOUT_MS = 9_000L
        const val MIN_POSITIVE_VERDICTS = 2
    }

    internal data class RequestHistorySnapshot(
        val requests: JsonArray,
        val revision: Long,
    )

    internal data class RequestContractReport(
        val positive: Int,
        val negative: Int,
        val malformed: Int,
        val failed: Int,
        val reason: String,
        val attemptedModels: List<String>,
    ) {
        val authorized: Boolean
            get() = positive >= MIN_POSITIVE_VERDICTS && negative == 0

        /** Status of an unauthorized report: any veto rejects, otherwise scope stays unverified. */
        val rejectionStatus: String
            get() = if (negative > 0) REQUEST_CONTRACT_REJECTED else REQUEST_CONTRACT_UNVERIFIED

        fun authorizedEvidence(): JsonObject = buildJsonObject {
            put("authorized", true)
            put("positive_verdicts", positive)
            put("negative_verdicts", negative)
            put("malformed_verdicts", malformed)
            put("failed_attempts", failed)
            put("reason", reason)
            put("attempted_model_ids", JsonArray(attemptedModels.map(::JsonPrimitive)))
        }
    }
}

internal typealias RequestContractReport = RequestContractChecker.RequestContractReport

internal const val REQUEST_CONTRACT_REJECTED = "request_contract_rejected"
internal const val REQUEST_CONTRACT_UNVERIFIED = "request_contract_unverified"
internal const val PROPOSAL_NOT_ASSESSABLE = "proposal_not_assessable"
private const val MAX_REQUEST_CONTEXT_BYTES = 96 * 1024

/** One checker context: the request history beside one proposal, or null when over budget. */
internal fun requestContractContext(
    history: JsonArray,
    proposalKey: String,
    proposal: JsonObject,
): String? = buildJsonObject {
    put("user_request_history", history)
    put(proposalKey, proposal)
}.toString().takeIf { it.toByteArray(Charsets.UTF_8).size <= MAX_REQUEST_CONTEXT_BYTES }

internal fun requestContractRejection(
    status: String,
    reason: String,
    report: RequestContractReport? = null,
): JsonObject = buildJsonObject {
    put("authorized", false)
    put("authorization_status", status)
    put("positive_verdicts", report?.positive ?: 0)
    put("negative_verdicts", report?.negative ?: 0)
    put("malformed_verdicts", report?.malformed ?: 0)
    put("failed_attempts", report?.failed ?: 0)
    put("reason", reason)
    put("original_unchanged", true)
    report?.let {
        put("attempted_model_ids", JsonArray(it.attemptedModels.map(::JsonPrimitive)))
    }
}

internal fun JsonObject.markedCached(): JsonObject = buildJsonObject {
    this@markedCached.forEach { (key, value) -> put(key, value) }
    put("cached", true)
}

internal fun requestContractLogFields(data: JsonObject): String =
    "positive=${data.verdictCount("positive_verdicts")} " +
        "negative=${data.verdictCount("negative_verdicts")} " +
        "malformed=${data.verdictCount("malformed_verdicts")}"

internal fun requestContractKey(context: String): String = MessageDigest.getInstance("SHA-256")
    .digest(context.toByteArray(Charsets.UTF_8))
    .joinToString("") { "%02x".format(it) }

/**
 * Final verdicts for one request-history revision. A history change makes every
 * entry stale, so a cached verdict never outlives the requests it judged.
 */
internal class RequestContractVerdictCache<D : Any>(private val capacity: Int) {
    private val lock = Any()
    private val entries = linkedMapOf<String, D>()
    private var revision: Long? = null

    fun get(revision: Long, key: String): D? = synchronized(lock) {
        syncLocked(revision)
        entries[key]
    }

    fun put(revision: Long, key: String, verdict: D) {
        synchronized(lock) {
            syncLocked(revision)
            entries[key] = verdict
            while (entries.size > capacity) entries.remove(entries.keys.first())
        }
    }

    private fun syncLocked(revision: Long) {
        if (this.revision == revision) return
        entries.clear()
        this.revision = revision
    }
}

private fun JsonObject.verdictCount(name: String): Int =
    (get(name) as? JsonPrimitive)?.contentOrNull?.toIntOrNull() ?: 0

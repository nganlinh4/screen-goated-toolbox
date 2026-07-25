package dev.screengoated.toolbox.mobile.phonecontrol.authorization

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.provider.FileMutationTargetLease
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlTurnRecorder
import java.io.File
import java.nio.file.Paths
import java.security.MessageDigest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

internal data class ResourceAuthorizationDecision(
    val authorized: Boolean,
    val code: String,
    val message: String,
    val data: JsonObject,
    val targetLease: FileMutationTargetLease?,
)

internal fun interface PhoneControlResourceAuthorizer {
    suspend fun evaluate(
        tool: String,
        arguments: JsonObject,
    ): ResourceAuthorizationDecision
}

/**
 * Independent semantic checkpoint for the exact durable file target.
 *
 * Kotlin validates only proposal shape and target identity. A separate model
 * quorum judges whether committed user requests grant mutation scope.
 */
internal class PhoneControlResourceAuthorization(
    private val modelIds: () -> List<String>,
    private val candidateClient: RequestContractCandidateClient,
) : PhoneControlTurnRecorder, PhoneControlResourceAuthorizer {
    constructor(context: Context) : this(AndroidRequestContractCandidates(context))

    private constructor(candidates: AndroidRequestContractCandidates) : this(
        modelIds = candidates::modelIds,
        candidateClient = candidates,
    )

    private data class RequestRecord(
        val turnId: Long,
        var text: String,
        var completed: Boolean,
    )

    private val lock = Any()
    private val requests = linkedMapOf<Long, RequestRecord>()
    private val cached = linkedMapOf<String, ResourceAuthorizationDecision>()

    override fun turnStarted(turnId: Long, generation: Long) {
        synchronized(lock) {
            val current = requests[turnId]
            requests.clear()
            requests[turnId] = current ?: RequestRecord(turnId, "", completed = false)
            cached.clear()
        }
    }

    override fun userTranscriptUpdated(turnId: Long, text: String) {
        val normalized = text.trim()
        if (normalized.isEmpty()) return
        synchronized(lock) {
            val request = requests.getOrPut(turnId) {
                RequestRecord(turnId, "", completed = false)
            }
            request.text = normalized
            trimHistoryLocked()
            cached.clear()
        }
    }

    override fun assistantTranscriptUpdated(turnId: Long, text: String) = Unit

    override fun turnCompleted(turnId: Long, userText: String, assistantText: String) {
        synchronized(lock) {
            val normalized = userText.trim()
            if (normalized.isNotEmpty()) {
                requests[turnId] = RequestRecord(turnId, normalized, completed = true)
            } else {
                requests.remove(turnId)
            }
            trimHistoryLocked()
            cached.clear()
        }
    }

    override fun turnInterrupted(turnId: Long) {
        synchronized(lock) {
            requests.remove(turnId)
            cached.clear()
        }
    }

    override suspend fun evaluate(
        tool: String,
        arguments: JsonObject,
    ): ResourceAuthorizationDecision {
        val prepared = synchronized(lock) { buildContextLocked(tool, arguments) }
            ?: return rejected(
                status = "proposal_not_assessable",
                reason = "No bounded user request and exact file-target proposal are available.",
            )
        val contextHash = sha256("${prepared.context}\n${prepared.lease.cacheIdentity()}")
        synchronized(lock) {
            cached[contextHash]?.let { decision ->
                return decision.copy(
                    data = buildJsonObject {
                        decision.data.forEach { (key, value) -> put(key, value) }
                        put("cached", true)
                    },
                )
            }
        }
        val candidates = runCatching { modelIds() }
            .getOrDefault(emptyList())
            .map(String::trim)
            .filter(String::isNotEmpty)
            .distinct()
        val report = withTimeoutOrNull(TOTAL_TIMEOUT_MS) {
            evaluateCandidates(candidates, prepared.context)
        }
        val decision = report?.toDecision(prepared.lease) ?: rejected(
            status = "request_contract_unverified",
            reason = "Independent target-scope checks did not finish within the bounded time.",
        )
        if (decision.authorized || decision.code == REJECTED_CODE) {
            synchronized(lock) {
                cached[contextHash] = decision
                while (cached.size > MAX_CACHED_PROPOSALS) {
                    cached.remove(cached.keys.first())
                }
            }
        }
        Log.i(
            TAG,
            "resource_scope authorized=${decision.authorized} " +
                "positive=${decision.data.intValue("positive_verdicts")} " +
                "negative=${decision.data.intValue("negative_verdicts")} " +
                "malformed=${decision.data.intValue("malformed_verdicts")}",
        )
        return decision
    }

    private suspend fun evaluateCandidates(
        candidates: List<String>,
        context: String,
    ): CandidateReport {
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
                    candidateClient.request(modelId, INSTRUCTION, context)
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
        return CandidateReport(
            positive = positive,
            negative = negative,
            malformed = malformed,
            failed = failed,
            reason = reason ?: "No independent model returned a valid authorization verdict.",
            attemptedModels = attempted,
        )
    }

    private fun buildContextLocked(
        tool: String,
        arguments: JsonObject,
    ): AuthorizationContext? {
        val history = requests.values.filter { it.text.isNotBlank() }
        if (history.isEmpty()) return null
        val proposal = targetProposal(tool, arguments) ?: return null
        val context = buildJsonObject {
            put(
                "user_request_history",
                buildJsonArray {
                    history.forEach { request ->
                        add(buildJsonObject {
                            put("turn_id", request.turnId)
                            put("text", request.text)
                        })
                    }
                },
            )
            put("proposed_resource_mutation", proposal.context)
        }.toString()
        return AuthorizationContext(context, proposal.lease)
    }

    private fun targetProposal(
        tool: String,
        arguments: JsonObject,
    ): TargetProposal? {
        val requested = arguments.stringValue("path")?.trim()?.takeIf(String::isNotEmpty)
            ?: return null
        val target = lexicalAbsolute(requested)?.canonicalFile ?: return null
        val existedBefore = target.isFile
        if (target.exists() && !existedBefore) return null
        val operation = when (tool) {
            "edit_text_file", "edit_text_file_structure" -> {
                if (!existedBefore || !validExactEditProposal(arguments)) return null
                "modify_existing_text_file"
            }
            "save_artifact" -> if (existedBefore) {
                "replace_existing_file"
            } else {
                "create_file"
            }
            else -> return null
        }
        val actualSha256 = if (existedBefore) target.sha256OrNull() ?: return null else null
        if (
            tool in EXACT_EDIT_TOOLS &&
            !actualSha256.equals(arguments.stringValue("expected_sha256"), ignoreCase = true)
        ) {
            return null
        }
        val lease = FileMutationTargetLease(
            canonicalPath = target.absolutePath,
            existedBefore = existedBefore,
            expectedSha256 = actualSha256,
        )
        return TargetProposal(
            context = buildJsonObject {
                put("capability_class", "dedicated_local_file_write")
                put("operation", operation)
                put("requested_path", requested)
                put("canonical_target", target.absolutePath)
                put("target_existed_before", existedBefore)
                put("overwrite_requested", arguments.booleanValue("overwrite") ?: false)
            },
            lease = lease,
        )
    }

    private fun validExactEditProposal(arguments: JsonObject): Boolean {
        val hash = arguments.stringValue("expected_sha256").orEmpty()
        if (hash.length != SHA256_HEX_CHARS || hash.any { !it.isHexDigit() }) return false
        val replacements = arguments["replacements"] as? JsonArray ?: return false
        if (replacements.isEmpty() || replacements.size > MAX_REPLACEMENT_GROUPS) return false
        return replacements.all { element ->
            val replacement = element as? JsonObject ?: return@all false
            replacement.stringValue("old_text")?.isNotEmpty() == true &&
                replacement.stringValue("new_text") != null &&
                replacement.intValueOrNull("expected_count")?.let { it > 0 } == true
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

    private fun CandidateReport.toDecision(
        lease: FileMutationTargetLease,
    ): ResourceAuthorizationDecision {
        val authorized = positive >= MIN_POSITIVE_VERDICTS && negative == 0
        return if (authorized) {
            ResourceAuthorizationDecision(
                authorized = true,
                code = "ok",
                message = reason,
                data = evidence(authorized = true),
                targetLease = lease,
            )
        } else {
            rejected(
                status = if (negative > 0) {
                    "request_contract_rejected"
                } else {
                    "request_contract_unverified"
                },
                reason = reason,
                report = this,
            )
        }
    }

    private fun CandidateReport.evidence(authorized: Boolean) = buildJsonObject {
        put("authorized", authorized)
        put("positive_verdicts", positive)
        put("negative_verdicts", negative)
        put("malformed_verdicts", malformed)
        put("failed_attempts", failed)
        put("reason", reason)
        put("attempted_model_ids", JsonArray(attemptedModels.map(::JsonPrimitive)))
    }

    private fun rejected(
        status: String,
        reason: String,
        report: CandidateReport? = null,
    ) = ResourceAuthorizationDecision(
        authorized = false,
        code = if (status == "request_contract_rejected") REJECTED_CODE else UNVERIFIED_CODE,
        message = reason,
        data = buildJsonObject {
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
        },
        targetLease = null,
    )

    private data class AuthorizationContext(
        val context: String,
        val lease: FileMutationTargetLease,
    )

    private data class TargetProposal(
        val context: JsonObject,
        val lease: FileMutationTargetLease,
    )

    private data class CandidateReport(
        val positive: Int,
        val negative: Int,
        val malformed: Int,
        val failed: Int,
        val reason: String,
        val attemptedModels: List<String>,
    )

    private companion object {
        const val TAG = "SGTPhoneControlResource"
        const val MAX_REQUEST_TURNS = 6
        const val MAX_REQUEST_CHARS = 12_000
        const val MAX_CACHED_PROPOSALS = 32
        const val MAX_REPLACEMENT_GROUPS = 64
        const val SHA256_HEX_CHARS = 64
        const val TOTAL_TIMEOUT_MS = 28_000L
        const val PROVIDER_TIMEOUT_MS = 9_000L
        const val MIN_POSITIVE_VERDICTS = 2
        const val REJECTED_CODE = "ERR_FILE_TARGET_REQUEST_CONTRACT_REJECTED"
        const val UNVERIFIED_CODE = "ERR_FILE_TARGET_REQUEST_CONTRACT_UNVERIFIED"
        const val INSTRUCTION =
            "Act as an independent request-contract checker. Decide whether the " +
                "user-authored request history authorizes mutating the exact local-file target " +
                "in the proposal. Reading, analyzing, or using a resource as input does not by " +
                "itself authorize modifying it. Permission may cover an exact resource, a " +
                "containing scope, a resource class, or an unambiguous broad mutation goal; a " +
                "literal path mention is not required. Earlier constraints remain binding " +
                "unless a later request clearly changes them. Judge target scope only, not " +
                "whether the proposed file contents are good. Return one JSON object only: " +
                "{\"authorized\":boolean,\"reason\":\"brief exact reason\"}."
        val EXACT_EDIT_TOOLS = setOf("edit_text_file", "edit_text_file_structure")
    }
}

private fun lexicalAbsolute(requested: String): File? = runCatching {
    val path = Paths.get(requested)
    if (!path.isAbsolute) return null
    var depth = 0
    path.forEach { component ->
        when (component.toString()) {
            "." -> Unit
            ".." -> {
                if (depth == 0) return null
                depth -= 1
            }
            else -> depth += 1
        }
    }
    path.normalize().toFile()
}.getOrNull()

private fun Char.isHexDigit(): Boolean =
    this in '0'..'9' || this in 'a'..'f' || this in 'A'..'F'

private fun JsonObject.stringValue(name: String): String? =
    (get(name) as? JsonPrimitive)?.contentOrNull

private fun JsonObject.booleanValue(name: String): Boolean? =
    (get(name) as? JsonPrimitive)?.booleanOrNull

private fun JsonObject.intValueOrNull(name: String): Int? =
    (get(name) as? JsonPrimitive)?.contentOrNull?.toIntOrNull()

private fun JsonObject.intValue(name: String): Int =
    intValueOrNull(name) ?: 0

private fun sha256(text: String): String = MessageDigest.getInstance("SHA-256")
    .digest(text.toByteArray(Charsets.UTF_8))
    .joinToString("") { "%02x".format(it) }

private fun File.sha256OrNull(): String? = runCatching {
    val digest = MessageDigest.getInstance("SHA-256")
    inputStream().use { input ->
        val buffer = ByteArray(HASH_BUFFER_BYTES)
        while (true) {
            val count = input.read(buffer)
            if (count < 0) break
            if (count > 0) digest.update(buffer, 0, count)
        }
    }
    digest.digest().joinToString("") { "%02x".format(it) }
}.getOrNull()

private fun FileMutationTargetLease.cacheIdentity(): String =
    "$canonicalPath|$existedBefore|${expectedSha256.orEmpty()}"

private const val HASH_BUFFER_BYTES = 64 * 1024

package dev.screengoated.toolbox.mobile.phonecontrol.authorization

import android.content.Context
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlTurnRecorder
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelType
import dev.screengoated.toolbox.mobile.preset.PresetRetryChainKind
import dev.screengoated.toolbox.mobile.preset.TextApiClient
import dev.screengoated.toolbox.mobile.preset.effectiveChain
import dev.screengoated.toolbox.mobile.preset.preflightSkipReason
import java.security.MessageDigest
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

internal data class StructuralAuthorizationDecision(
    val authorized: Boolean,
    val code: String,
    val message: String,
    val data: JsonObject,
)

internal fun interface RequestContractCandidateClient {
    suspend fun request(
        modelId: String,
        instruction: String,
        context: String,
    ): Result<String>
}

internal class PhoneControlStructuralEditAuthorization(
    private val modelIds: () -> List<String>,
    private val candidateClient: RequestContractCandidateClient,
) : PhoneControlTurnRecorder {
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
    private var cached: Pair<String, StructuralAuthorizationDecision>? = null

    override fun turnStarted(turnId: Long, generation: Long) {
        synchronized(lock) {
            val current = requests[turnId]
            requests.clear()
            requests[turnId] = current ?: RequestRecord(turnId, "", completed = false)
            cached = null
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
            cached = null
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
            cached = null
        }
    }

    override fun turnInterrupted(turnId: Long) {
        synchronized(lock) {
            requests.remove(turnId)
            cached = null
        }
    }

    suspend fun evaluate(
        args: JsonObject,
        preflight: JsonObject,
    ): StructuralAuthorizationDecision {
        val context = synchronized(lock) { buildContextLocked(args, preflight) }
            ?: return rejected(
                code = UNVERIFIED_CODE,
                status = "proposal_not_assessable",
                reason = "No bounded user-authored request history can assess this proposal.",
            )
        val contextHash = sha256(context)
        synchronized(lock) {
            cached?.takeIf { it.first == contextHash }?.second?.let { decision ->
                return decision.copy(
                    data = buildJsonObject {
                        decision.data.forEach { (key, value) -> put(key, value) }
                        put("cached", true)
                    },
                )
            }
        }
        val candidates = modelIds().map(String::trim).filter(String::isNotEmpty).distinct()
        val report = withTimeoutOrNull(TOTAL_TIMEOUT_MS) {
            evaluateCandidates(candidates, context)
        }
        val decision = report?.toDecision() ?: rejected(
            code = UNVERIFIED_CODE,
            status = "request_contract_unverified",
            reason = "Independent request-contract checks did not finish within the bounded time.",
        )
        if (decision.authorized || decision.code == REJECTED_CODE) {
            synchronized(lock) { cached = contextHash to decision }
        }
        Log.i(
            TAG,
            "structural_request_contract authorized=${decision.authorized} " +
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
            val answer = response.getOrNull()
            if (answer == null) {
                failed += 1
                continue
            }
            val verdict = parseRequestContractVerdict(answer)
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
        args: JsonObject,
        preflight: JsonObject,
    ): String? {
        val history = requests.values.filter { it.text.isNotBlank() }
        if (history.isEmpty()) return null
        val replacements = args["replacements"] as? JsonArray ?: return null
        var proposalChars = 0
        val changes = buildJsonArray {
            replacements.forEachIndexed { index, element ->
                val replacement = element as? JsonObject ?: return null
                val oldText = replacement.stringValue("old_text") ?: return null
                val newText = replacement.stringValue("new_text") ?: return null
                proposalChars += oldText.length + newText.length
                if (proposalChars > MAX_PROPOSAL_CHARS) return null
                add(
                    buildJsonObject {
                        put("index", index)
                        replacement["expected_count"]?.let { put("expected_count", it) }
                        put("old_text", oldText)
                        put("new_text", newText)
                    },
                )
            }
        }
        return buildJsonObject {
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
            put(
                "proposed_structural_change",
                buildJsonObject {
                    args["path"]?.let { put("path", it) }
                    put("replacement_text_chars", proposalChars)
                    put("replacements", changes)
                    put("preflight", boundedPreflight(preflight))
                },
            )
        }.toString()
    }

    private fun trimHistoryLocked() {
        requests.entries.removeAll { it.value.text.isBlank() && it.value.completed }
        while (
            requests.size > MAX_REQUEST_TURNS ||
            requests.values.sumOf { it.text.length } > MAX_REQUEST_CHARS
        ) {
            val first = requests.keys.firstOrNull() ?: break
            requests.remove(first)
        }
    }

    private fun CandidateReport.toDecision(): StructuralAuthorizationDecision {
        val authorized = positive >= MIN_POSITIVE_VERDICTS && negative == 0
        return if (authorized) {
            StructuralAuthorizationDecision(
                authorized = true,
                code = "ok",
                message = reason,
                data = evidence(authorized = true),
            )
        } else {
            rejected(
                code = if (negative > 0) REJECTED_CODE else UNVERIFIED_CODE,
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
        code: String,
        status: String,
        reason: String,
        report: CandidateReport? = null,
    ) = StructuralAuthorizationDecision(
        authorized = false,
        code = code,
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
        const val TAG = "SGTPhoneControlStructure"
        const val MAX_REQUEST_TURNS = 6
        const val MAX_REQUEST_CHARS = 12_000
        const val MAX_PROPOSAL_CHARS = 16_000
        const val TOTAL_TIMEOUT_MS = 28_000L
        const val PROVIDER_TIMEOUT_MS = 9_000L
        const val MIN_POSITIVE_VERDICTS = 2
        const val REJECTED_CODE = "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED"
        const val UNVERIFIED_CODE = "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_UNVERIFIED"
        const val INSTRUCTION =
            "Act as an independent request-contract checker. Decide whether the " +
                "user-authored request history explicitly authorizes the exact proposed CSV/TSV " +
                "record-shape or formula-cell change. Ordinary data updates do not imply " +
                "permission to remove headers, change row/column shape, or rewrite formulas. " +
                "Earlier user constraints remain binding unless a later user request clearly " +
                "changes them. Authorize only when the structural effect itself is directly " +
                "requested and not contradicted. Return one JSON object only: " +
                "{\"authorized\":boolean,\"reason\":\"brief exact reason\"}."
    }
}

internal class AndroidRequestContractCandidates(context: Context) :
    RequestContractCandidateClient {
    private val container =
        (context.applicationContext as SgtMobileApplication).appContainer
    private val client = TextApiClient(
        container.httpClient.newBuilder()
            .connectTimeout(PROVIDER_TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .readTimeout(PROVIDER_TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .writeTimeout(PROVIDER_TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .callTimeout(PROVIDER_TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .build(),
    )

    fun modelIds(): List<String> {
        val settings = container.currentPresetRuntimeSettings()
        val apiKeys = container.currentPresetApiKeys()
        return PresetRetryChainKind.TEXT_TO_TEXT.effectiveChain(settings, apiKeys)
            .distinct()
            .filter { id ->
                val model = PresetModelCatalog.getById(id)
                model?.modelType == PresetModelType.TEXT &&
                    preflightSkipReason(
                        modelId = id,
                        provider = model.provider,
                        apiKeys = apiKeys,
                        blockedProviders = emptySet(),
                        settings = settings,
                    ) == null
            }
    }

    override suspend fun request(
        modelId: String,
        instruction: String,
        context: String,
    ): Result<String> = client.executeStreaming(
        modelId = modelId,
        prompt = instruction,
        inputText = context,
        apiKeys = container.currentPresetApiKeys(),
        uiLanguage = "en",
        searchLabel = null,
        onChunk = {},
        streamingEnabled = false,
    )

    private companion object {
        const val PROVIDER_TIMEOUT_SECONDS = 9L
    }
}

internal data class RequestContractVerdict(
    val authorized: Boolean,
    val reason: String,
)

internal fun parseRequestContractVerdict(answer: String): RequestContractVerdict? =
    balancedJsonObjects(answer).take(MAX_JSON_CANDIDATES).firstNotNullOfOrNull { candidate ->
        val value = runCatching { STRICT_JSON.parseToJsonElement(candidate).jsonObject }.getOrNull()
            ?: return@firstNotNullOfOrNull null
        val authorized = value["authorized"]?.jsonPrimitive?.booleanOrNull
            ?: return@firstNotNullOfOrNull null
        val reason = value["reason"]?.jsonPrimitive?.contentOrNull?.trim().orEmpty()
        reason.takeIf(String::isNotEmpty)?.let {
            RequestContractVerdict(authorized, it.take(MAX_REASON_CHARS))
        }
    }

private fun balancedJsonObjects(text: String): Sequence<String> = sequence {
    text.indices.asSequence().filter { text[it] == '{' }.forEach { start ->
        var depth = 0
        var inString = false
        var escaped = false
        for (index in start until text.length) {
            val character = text[index]
            if (inString) {
                when {
                    escaped -> escaped = false
                    character == '\\' -> escaped = true
                    character == '"' -> inString = false
                }
            } else {
                when (character) {
                    '"' -> inString = true
                    '{' -> depth += 1
                    '}' -> {
                        depth -= 1
                        if (depth == 0) {
                            yield(text.substring(start, index + 1))
                            break
                        }
                    }
                }
            }
        }
    }
}

private fun boundedPreflight(preflight: JsonObject): JsonObject {
    val structure = preflight["structure"] as? JsonObject
    return buildJsonObject {
        PREFLIGHT_FIELDS.forEach { field ->
            (preflight[field] ?: structure?.get(field))?.let { put(field, it) }
        }
    }
}

private fun JsonObject.stringValue(name: String): String? =
    (get(name) as? JsonPrimitive)?.contentOrNull

private fun JsonObject.intValue(name: String): Int =
    (get(name) as? JsonPrimitive)?.contentOrNull?.toIntOrNull() ?: 0

private fun sha256(text: String): String = MessageDigest.getInstance("SHA-256")
    .digest(text.toByteArray(Charsets.UTF_8))
    .joinToString("") { "%02x".format(it) }

private val STRICT_JSON = Json { ignoreUnknownKeys = true }
private const val MAX_JSON_CANDIDATES = 64
private const val MAX_REASON_CHARS = 1_024
private val PREFLIGHT_FIELDS = listOf(
    "code",
    "format",
    "before_record_count",
    "after_record_count",
    "before_formula_count",
    "after_formula_count",
    "before_field_counts",
    "after_field_counts",
    "parse_error",
)

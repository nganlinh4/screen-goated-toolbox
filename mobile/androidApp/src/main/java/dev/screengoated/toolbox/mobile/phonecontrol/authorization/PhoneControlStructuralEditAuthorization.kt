package dev.screengoated.toolbox.mobile.phonecontrol.authorization

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlTurnRecorder
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.put

internal data class StructuralAuthorizationDecision(
    val authorized: Boolean,
    val code: String,
    val message: String,
    val data: JsonObject,
)

/**
 * Independent checkpoint for exact CSV/TSV record-shape or formula changes.
 * It shares the [RequestContractChecker] history and quorum with resource
 * authorization and owns only its proposal context and result codes.
 */
internal class PhoneControlStructuralEditAuthorization private constructor(
    private val checker: RequestContractChecker,
) : PhoneControlTurnRecorder by checker {
    constructor(
        modelIds: () -> List<String>,
        candidateClient: RequestContractCandidateClient,
    ) : this(RequestContractChecker(modelIds, candidateClient))

    constructor(context: Context) : this(AndroidRequestContractCandidates(context))

    private constructor(candidates: AndroidRequestContractCandidates) : this(
        modelIds = candidates::modelIds,
        candidateClient = candidates,
    )

    private val cache = RequestContractVerdictCache<StructuralAuthorizationDecision>(capacity = 1)

    suspend fun evaluate(
        args: JsonObject,
        preflight: JsonObject,
    ): StructuralAuthorizationDecision {
        val history = checker.history() ?: return notAssessable()
        val proposal = structuralProposal(args, preflight) ?: return notAssessable()
        val context = requestContractContext(history.requests, "proposed_structural_change", proposal)
            ?: return notAssessable()
        val contextHash = requestContractKey(context)
        cache.get(history.revision, contextHash)?.let { decision ->
            return decision.copy(data = decision.data.markedCached())
        }
        val decision = checker.assess(INSTRUCTION, context)?.toDecision() ?: rejected(
            status = REQUEST_CONTRACT_UNVERIFIED,
            reason = "Independent request-contract checks did not finish within the bounded time.",
        )
        if (!checker.isCurrent(history.revision)) {
            return rejected(REQUEST_CONTRACT_UNVERIFIED, "The committed request changed during assessment.")
        }
        if (decision.authorized || decision.code == REJECTED_CODE) {
            cache.put(history.revision, contextHash, decision)
        }
        Log.i(
            TAG,
            "structural_request_contract authorized=${decision.authorized} " +
                requestContractLogFields(decision.data),
        )
        return decision
    }

    private fun structuralProposal(
        args: JsonObject,
        preflight: JsonObject,
    ): JsonObject? {
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
            args["path"]?.let { put("path", it) }
            put("replacement_text_chars", proposalChars)
            put("replacements", changes)
            put("preflight", boundedPreflight(preflight))
        }
    }

    private fun RequestContractReport.toDecision(): StructuralAuthorizationDecision = if (authorized) {
        StructuralAuthorizationDecision(
            authorized = true,
            code = "ok",
            message = reason,
            data = authorizedEvidence(),
        )
    } else {
        rejected(status = rejectionStatus, reason = reason, report = this)
    }

    private fun notAssessable() = rejected(
        PROPOSAL_NOT_ASSESSABLE,
        "No bounded user-authored request history can assess this proposal.",
    )

    private fun rejected(
        status: String,
        reason: String,
        report: RequestContractReport? = null,
    ) = StructuralAuthorizationDecision(
        authorized = false,
        code = if (status == REQUEST_CONTRACT_REJECTED) REJECTED_CODE else UNVERIFIED_CODE,
        message = reason,
        data = requestContractRejection(status, reason, report),
    )

    private companion object {
        const val TAG = "SGTPhoneControlStructure"
        const val MAX_PROPOSAL_CHARS = 16_000
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

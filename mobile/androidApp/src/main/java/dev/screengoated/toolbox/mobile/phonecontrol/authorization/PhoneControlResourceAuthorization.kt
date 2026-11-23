package dev.screengoated.toolbox.mobile.phonecontrol.authorization

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.provider.FileMutationTargetLease
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlTurnRecorder
import kotlinx.serialization.json.JsonObject

internal data class ResourceAuthorizationDecision(
    val authorized: Boolean,
    val code: String,
    val message: String,
    val data: JsonObject,
    val targetLease: FileMutationTargetLease?,
    val isCurrent: () -> Boolean = { true },
)

internal fun interface PhoneControlResourceAuthorizer {
    suspend fun evaluate(
        tool: String,
        mutation: PreparedResourceMutation,
    ): ResourceAuthorizationDecision

    suspend fun evaluateBoundary(proposal: JsonObject): ResourceAuthorizationDecision =
        unavailableResourceBoundary("The independent resource-effect checker is unavailable.")
}

/**
 * Independent semantic checkpoint for the exact durable file target.
 *
 * Kotlin validates only proposal shape and target identity. The shared
 * [RequestContractChecker] quorum judges whether committed user requests grant
 * mutation scope.
 */
internal class PhoneControlResourceAuthorization private constructor(
    private val checker: RequestContractChecker,
) : PhoneControlTurnRecorder by checker, PhoneControlResourceAuthorizer {
    constructor(
        modelIds: () -> List<String>,
        candidateClient: RequestContractCandidateClient,
    ) : this(RequestContractChecker(modelIds, candidateClient))

    constructor(context: Context) : this(AndroidRequestContractCandidates(context))

    private constructor(candidates: AndroidRequestContractCandidates) : this(
        modelIds = candidates::modelIds,
        candidateClient = candidates,
    )

    private val cache = RequestContractVerdictCache<ResourceAuthorizationDecision>(MAX_CACHED_PROPOSALS)

    override suspend fun evaluate(
        tool: String,
        mutation: PreparedResourceMutation,
    ): ResourceAuthorizationDecision {
        val history = checker.history()?.takeIf { tool in EXACT_EDIT_TOOLS || tool == "save_artifact" }
            ?: return notAssessable()
        val context = requestContractContext(history.requests, "proposed_resource_mutation", mutation.proposal)
            ?: return notAssessable()
        val isCurrent = { checker.isCurrent(history.revision) }
        val contextHash = requestContractKey(context)
        cache.get(history.revision, contextHash)?.let { decision ->
            return decision.copy(data = decision.data.markedCached(), isCurrent = isCurrent)
        }
        val decision = checker.assess(INSTRUCTION, context)?.toDecision(mutation.targetLease) ?: rejected(
            status = REQUEST_CONTRACT_UNVERIFIED,
            reason = "Independent target-scope checks did not finish within the bounded time.",
        )
        if (!isCurrent()) return changedDuringAssessment()
        if (decision.authorized || decision.code == REJECTED_CODE) {
            cache.put(history.revision, contextHash, decision)
        }
        Log.i(
            TAG,
            "resource_scope authorized=${decision.authorized} ${requestContractLogFields(decision.data)}",
        )
        return decision.copy(isCurrent = isCurrent)
    }

    override suspend fun evaluateBoundary(proposal: JsonObject): ResourceAuthorizationDecision {
        val history = checker.history()
            ?: return unavailableResourceBoundary("No committed user request is available.")
        val context = requestContractContext(history.requests, "proposed_resource_boundary_effect", proposal)
            ?: return unavailableResourceBoundary("The complete resource-effect proposal exceeds its bounded budget.")
        val report = checker.assess(RESOURCE_BOUNDARY_INSTRUCTION, context)
            ?: return unavailableResourceBoundary("Independent resource-effect checks did not finish.")
        if (!checker.isCurrent(history.revision)) {
            return unavailableResourceBoundary("The committed request changed during assessment.")
        }
        // Generic actions depend on mutable UI, processes and integration state.
        // Every assessment is fresh, even when its arguments are identical.
        return report.toDecision(null).asResourceBoundaryDecision()
    }

    private fun RequestContractReport.toDecision(
        lease: FileMutationTargetLease?,
    ): ResourceAuthorizationDecision = if (authorized) {
        ResourceAuthorizationDecision(
            authorized = true,
            code = "ok",
            message = reason,
            data = authorizedEvidence(),
            targetLease = lease,
        )
    } else {
        rejected(status = rejectionStatus, reason = reason, report = this)
    }

    private fun notAssessable() = rejected(
        PROPOSAL_NOT_ASSESSABLE,
        "No bounded user request and exact file-target proposal are available.",
    )

    private fun changedDuringAssessment() = rejected(
        REQUEST_CONTRACT_UNVERIFIED,
        "The committed request changed during assessment.",
    )

    private fun rejected(
        status: String,
        reason: String,
        report: RequestContractReport? = null,
    ) = ResourceAuthorizationDecision(
        authorized = false,
        code = if (status == REQUEST_CONTRACT_REJECTED) REJECTED_CODE else UNVERIFIED_CODE,
        message = reason,
        data = requestContractRejection(status, reason, report),
        targetLease = null,
    )

    private companion object {
        const val TAG = "SGTPhoneControlResource"
        const val MAX_CACHED_PROPOSALS = 32
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

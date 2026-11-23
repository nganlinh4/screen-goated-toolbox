package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlResourceAuthorizer
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PreparedResourceMutation
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal suspend fun executeResourceScopedMutation(
    tool: String,
    prepared: PreparedResourceMutation,
    authorizer: PhoneControlResourceAuthorizer,
    mutation: suspend (() -> Boolean) -> AndroidProviderResult,
): AndroidProviderResult {
    val decision = authorizer.evaluate(tool, prepared)
    if (!decision.authorized) {
        return AndroidProviderResult.Failure(
            code = decision.code,
            message = decision.message,
            retryable = true,
            data = decision.data,
        )
    }
    val lease = decision.targetLease ?: return AndroidProviderResult.Failure(
        code = "ERR_FILE_TARGET_AUTHORIZATION_LEASE_MISSING",
        message = "The authorized file target has no commit lease.",
        retryable = true,
        data = buildJsonObject {
            decision.data.forEach { (key, value) -> put(key, value) }
            put("original_unchanged", true)
        },
    )
    if (lease != prepared.targetLease || !decision.isCurrent()) return AndroidProviderResult.Failure(
        code = "ERR_FILE_TARGET_AUTHORIZATION_LEASE_CHANGED",
        message = "The approved target or request changed before mutation.",
    )
    currentCoroutineContext().ensureActive()
    return mutation(decision.isCurrent).withMutationEvidence("resource_scope_authorization", decision.data)
}

internal fun AndroidProviderResult.withMutationEvidence(
    field: String,
    evidence: JsonObject,
): AndroidProviderResult {
    val original = when (this) {
        is AndroidProviderResult.Success -> data
        is AndroidProviderResult.Failure -> data
    }
    val attributed = buildJsonObject {
        original.forEach { (key, value) -> put(key, value) }
        put(field, evidence)
    }
    return when (this) {
        is AndroidProviderResult.Success -> copy(data = attributed)
        is AndroidProviderResult.Failure -> copy(data = attributed)
    }
}

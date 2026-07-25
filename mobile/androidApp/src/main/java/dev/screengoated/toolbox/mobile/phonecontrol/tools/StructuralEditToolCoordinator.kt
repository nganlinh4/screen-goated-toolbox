package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlResourceAuthorizer
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlStructuralEditAuthorization
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidFileProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class StructuralEditToolCoordinator(
    private val files: AndroidFileProvider,
    private val structuralAuthorization: PhoneControlStructuralEditAuthorization,
    private val resourceAuthorization: PhoneControlResourceAuthorizer,
) {
    suspend fun execute(
        args: JsonObject,
        request: ExactEditRequest,
    ): AndroidProviderResult {
        val token = args.string("structural_change_token")?.takeIf(String::isNotBlank)
        val preflight = files.structuralPreflight(
            request.path,
            request.expectedSha256,
            request.replacements,
            token,
        )
        if (preflight is AndroidProviderResult.Failure) return preflight

        val preflightData = (preflight as AndroidProviderResult.Success).data
        return executeResourceScopedMutation(
            tool = "edit_text_file_structure",
            arguments = args,
            authorizer = resourceAuthorization,
        ) { targetLease ->
            val decision = structuralAuthorization.evaluate(args, preflightData)
            if (!decision.authorized) {
                AndroidProviderResult.Failure(
                    code = decision.code,
                    message = decision.message,
                    retryable = true,
                    data = buildJsonObject {
                        decision.data.forEach { (key, value) -> put(key, value) }
                        put("preflight", preflightData)
                    },
                )
            } else {
                files.commitStructuralAfterAuthorization(
                    request.path,
                    request.expectedSha256,
                    request.replacements,
                    checkNotNull(token) {
                        "A successful structural preflight must attest its supplied token"
                    },
                    targetLease,
                ).withMutationEvidence(
                    "request_contract_authorization",
                    decision.data,
                )
            }
        }
    }
}

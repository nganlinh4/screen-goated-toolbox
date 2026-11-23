package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put

/**
 * Mirrors Windows `document_binding::MAIN_WORLD_CALL`. Indirect eval keeps the
 * global-scope semantics that `Runtime.evaluate` gives the same code.
 */
internal const val CDP_DOCUMENT_BOUND_CALL =
    "async function(code) { return await (0, eval)(code); }"

internal sealed interface CdpEvalResult {
    data class Value(val value: JsonElement) : CdpEvalResult
    data class Failure(val outcome: BrowserProviderOutcome) : CdpEvalResult
}

private sealed interface CdpDocumentObject {
    data class Resolved(val objectId: String) : CdpDocumentObject
    data class Failed(val outcome: BrowserProviderOutcome) : CdpDocumentObject
}

/**
 * Evaluates on one resolved document object, as Windows `eval_main_world`
 * does. The loader check runs after that object is resolved, and a navigation
 * after the check destroys the object's context, so the call is rejected
 * instead of running in the replacement page.
 */
internal suspend fun ChromeCdpController.evaluateOnBoundDocument(
    current: ChromeCdpController.BoundTarget,
    expression: String,
    userGesture: Boolean,
): CdpEvalResult {
    val objectId = when (val document = resolveDocumentObject(current)) {
        is CdpDocumentObject.Failed -> return CdpEvalResult.Failure(document.outcome)
        is CdpDocumentObject.Resolved -> document.objectId
    }
    try {
        ensureCurrentDocument(current)?.let { return CdpEvalResult.Failure(it) }
        val command = current.session.send(
            "Runtime.callFunctionOn",
            buildJsonObject {
                put("objectId", objectId)
                put("functionDeclaration", CDP_DOCUMENT_BOUND_CALL)
                put("arguments", buildJsonArray { add(buildJsonObject { put("value", expression) }) })
                put("returnByValue", true)
                put("awaitPromise", true)
                put("userGesture", userGesture)
            },
        )
        if (command is CdpCommandResult.Failure && command.code == CDP_COMMAND_REJECTED) {
            ensureCurrentDocument(current)?.let { return CdpEvalResult.Failure(it) }
        }
        return cdpEvalResult(command, userGesture)
    } finally {
        runCatching {
            current.session.send("Runtime.releaseObject", buildJsonObject { put("objectId", objectId) })
        }
    }
}

internal fun ChromeCdpController.cdpEvalResult(
    command: CdpCommandResult,
    userGesture: Boolean,
): CdpEvalResult {
    if (command is CdpCommandResult.Failure) return CdpEvalResult.Failure(command.toOutcome())
    val result = (command as CdpCommandResult.Success).result
    if (result["exceptionDetails"] != null) {
        return CdpEvalResult.Failure(
            failure(
                "browser_eval_exception",
                CapabilityState.READY,
                "The page expression threw an exception.",
                effect = if (userGesture) {
                    EffectCertainty.MAY_HAVE_OCCURRED
                } else {
                    EffectCertainty.PROVEN_NO_EFFECT
                },
                snapshotInvalidated = userGesture,
                retryable = false,
            ),
        )
    }
    val remote = result["result"] as? JsonObject
    return CdpEvalResult.Value(remote?.get("value") ?: JsonNull)
}

private suspend fun ChromeCdpController.resolveDocumentObject(
    current: ChromeCdpController.BoundTarget,
): CdpDocumentObject {
    val document = current.session.send("DOM.getDocument", buildJsonObject { put("depth", 0) })
    if (document is CdpCommandResult.Failure) return CdpDocumentObject.Failed(document.toOutcome())
    val backendNodeId = ((document as CdpCommandResult.Success).result["root"] as? JsonObject)
        ?.get("backendNodeId")
        ?.jsonPrimitive
        ?.longOrNull
        ?.takeIf { it > 0 }
        ?: return CdpDocumentObject.Failed(documentUnavailable())
    val resolved = current.session.send(
        "DOM.resolveNode",
        buildJsonObject { put("backendNodeId", backendNodeId) },
    )
    if (resolved is CdpCommandResult.Failure) return CdpDocumentObject.Failed(resolved.toOutcome())
    val objectId = ((resolved as CdpCommandResult.Success).result["object"] as? JsonObject)
        ?.get("objectId")
        ?.jsonPrimitive
        ?.contentOrNull
        ?.takeIf(String::isNotEmpty)
        ?: return CdpDocumentObject.Failed(documentUnavailable())
    return CdpDocumentObject.Resolved(objectId)
}

private fun ChromeCdpController.documentUnavailable(): BrowserProviderOutcome = failure(
    "browser_dom_unavailable",
    CapabilityState.DEGRADED,
    "Chrome returned no document root.",
)

private const val CDP_COMMAND_REJECTED = "browser_cdp_command_rejected"

package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

internal sealed interface BrowserTunnelResult {
    data class Ready(
        val leaseId: String,
        val port: Int,
        val bearerToken: String,
    ) : BrowserTunnelResult

    data class Failure(
        val state: CapabilityState,
        val code: String,
        val requiredUserStep: String?,
    ) : BrowserTunnelResult
}

internal fun parseBrowserTunnelResult(
    raw: String,
    fallbackGuidance: String?,
): BrowserTunnelResult {
    val data = Json.parseToJsonElement(raw).jsonObject
    val state = data["state"]?.jsonPrimitive?.contentOrNull
        ?.let { wire -> CapabilityState.entries.firstOrNull { it.wireName == wire } }
        ?: CapabilityState.DEGRADED
    val code = data["code"]?.jsonPrimitive?.contentOrNull.orEmpty()
    val leaseId = data["lease_id"]?.jsonPrimitive?.contentOrNull
    val port = data["port"]?.jsonPrimitive?.intOrNull
    val token = data["bearer_token"]?.jsonPrimitive?.contentOrNull
    return if (
        state == CapabilityState.READY &&
        !leaseId.isNullOrBlank() &&
        port != null &&
        port in 1..65_535 &&
        token != null &&
        token.length in MIN_TUNNEL_TOKEN_CHARS..MAX_TUNNEL_TOKEN_CHARS &&
        token.all { it.isLetterOrDigit() || it == '-' || it == '_' }
    ) {
        BrowserTunnelResult.Ready(leaseId, port, token)
    } else {
        BrowserTunnelResult.Failure(
            state = state,
            code = code.ifBlank { "browser_tunnel_invalid" },
            requiredUserStep = fallbackGuidance,
        )
    }
}

private const val MIN_TUNNEL_TOKEN_CHARS = 32
private const val MAX_TUNNEL_TOKEN_CHARS = 128

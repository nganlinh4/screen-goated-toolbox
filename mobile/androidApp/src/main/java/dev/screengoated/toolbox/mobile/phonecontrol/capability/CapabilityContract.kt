package dev.screengoated.toolbox.mobile.phonecontrol.capability

/** Stable capability states shared with the Phone Control authority fixture. */
internal enum class CapabilityState(val wireName: String) {
    READY("ready"),
    DEGRADED("degraded"),
    NEEDS_USER_STEP("needs_user_step"),
    REVOKED("revoked"),
    UNSUPPORTED("unsupported"),
    UNAVAILABLE("unavailable"),
    ;

    val isReadyForRouting: Boolean
        get() = this == READY

    companion object {
        fun fromWireName(value: String): CapabilityState =
            entries.firstOrNull { it.wireName == value }
                ?: throw IllegalArgumentException("Unknown capability state: $value")
    }
}

/** Provider identity is data, not a privilege ranking used by the router. */
internal data class ProviderDefinition(
    val id: String,
    val authority: String,
    val optional: Boolean,
) {
    init {
        require(id.isNotBlank()) { "provider id must not be blank" }
        require(authority.isNotBlank()) { "provider authority must not be blank" }
    }
}

/** Ordered candidates for one exact capability. Earlier providers are preferred. */
internal data class CapabilityRoute(
    val capability: String,
    val providerIds: List<String>,
) {
    init {
        require(capability.isNotBlank()) { "capability must not be blank" }
        require(providerIds.isNotEmpty()) { "capability route must name a provider" }
        require(providerIds.none(String::isBlank)) { "provider ids must not be blank" }
        require(providerIds.distinct().size == providerIds.size) {
            "capability route must not repeat a provider"
        }
    }
}

/**
 * One runtime probe. A provider may advertise several capabilities, each with
 * semantic facets that describe the complete behavior it can supply.
 */
internal class ProviderSnapshot(
    val providerId: String,
    val state: CapabilityState,
    supportedCapabilities: Map<String, Set<String>>,
    val evidenceTimestampMs: Long,
    val requiredUserStep: String? = null,
) {
    val supportedCapabilities: Map<String, Set<String>> = supportedCapabilities
        .mapValues { (_, semantics) -> semantics.filter(String::isNotBlank).toSet() }
        .toMap()

    init {
        require(providerId.isNotBlank()) { "provider id must not be blank" }
        require(evidenceTimestampMs >= 0) { "evidence timestamp must be non-negative" }
        require(this.supportedCapabilities.keys.none(String::isBlank)) {
            "supported capability ids must not be blank"
        }
        require(requiredUserStep == null || requiredUserStep.isNotBlank()) {
            "required user step must be absent or non-blank"
        }
    }

    fun supplies(request: CapabilityRequest): Boolean {
        val suppliedSemantics = supportedCapabilities[request.capability] ?: return false
        return suppliedSemantics.containsAll(request.requiredSemantics)
    }
}

/** The requested tool remains unchanged while providers are considered. */
internal data class CapabilityRequest(
    val capability: String,
    val requestedTool: String,
    val requiredSemantics: Set<String> = emptySet(),
) {
    init {
        require(capability.isNotBlank()) { "capability must not be blank" }
        require(requestedTool.isNotBlank()) { "requested tool must not be blank" }
        require(requiredSemantics.none(String::isBlank)) {
            "required semantics must not contain blank values"
        }
    }
}

internal object PhoneControlCapabilityContract {
    const val NORMAL_TURN_POLICY = "stable_full_catalog"
    const val UNAVAILABLE_RESULT_CODE = "capability_unavailable"
    const val DYNAMIC_HIDING = false
    const val PHRASE_OR_LANGUAGE_GATES = false
    const val SILENT_TOOL_REROUTES = false
}

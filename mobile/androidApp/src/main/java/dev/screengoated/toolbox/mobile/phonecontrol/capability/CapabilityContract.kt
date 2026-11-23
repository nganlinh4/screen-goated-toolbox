package dev.screengoated.toolbox.mobile.phonecontrol.capability

/** Stable capability states shared with the Phone Control authority fixture. */
internal enum class CapabilityState(val wireName: String) {
    READY("ready"),
    DEGRADED("degraded"),
    NEEDS_USER_STEP("needs_user_step"),
    REVOKED("revoked"),
    UNSUPPORTED("unsupported"),
    UNAVAILABLE("unavailable"),
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

/** One runtime readiness probe of a provider, reported in the execution context. */
internal class ProviderSnapshot(
    val providerId: String,
    val state: CapabilityState,
    val evidenceTimestampMs: Long,
    val requiredUserStep: String? = null,
) {
    init {
        require(providerId.isNotBlank()) { "provider id must not be blank" }
        require(evidenceTimestampMs >= 0) { "evidence timestamp must be non-negative" }
        require(requiredUserStep == null || requiredUserStep.isNotBlank()) {
            "required user step must be absent or non-blank"
        }
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

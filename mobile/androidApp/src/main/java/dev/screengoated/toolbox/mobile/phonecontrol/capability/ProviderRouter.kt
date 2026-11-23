package dev.screengoated.toolbox.mobile.phonecontrol.capability

internal enum class ProviderPlanRejection {
    CAPABILITY_NOT_ROUTED,
    PROVIDER_NOT_DECLARED,
    PROVIDER_OUTSIDE_CAPABILITY_ROUTE,
    PROVIDER_ORDER_MISMATCH,
}

internal data class ProviderExecutionPlan(
    val request: CapabilityRequest,
    val providers: List<ProviderDefinition>,
) {
    init {
        require(providers.isNotEmpty()) { "provider execution plan must not be empty" }
        require(providers.distinctBy(ProviderDefinition::id).size == providers.size) {
            "provider execution plan must not repeat providers"
        }
    }
}

internal sealed interface ProviderExecutionPlanDecision {
    data class Planned(val plan: ProviderExecutionPlan) : ProviderExecutionPlanDecision

    data class Invalid(
        val request: CapabilityRequest,
        val rejection: ProviderPlanRejection,
        val providerId: String? = null,
    ) : ProviderExecutionPlanDecision
}

internal sealed interface ProviderReceiptDecision {
    data class Accepted(
        val provider: ProviderDefinition,
        val priorityIndex: Int,
    ) : ProviderReceiptDecision

    data class Rejected(val providerId: String) : ProviderReceiptDecision
}

/**
 * Plans the exact provider allowlist for one requested tool on its capability
 * route and attests which provider produced the receipt. It never searches a
 * different capability or rewrites the requested tool.
 */
internal class ProviderRouter(
    providers: List<ProviderDefinition>,
    routes: List<CapabilityRoute>,
) {
    private val providersById: Map<String, ProviderDefinition> = providers.associateBy { it.id }
    private val routesByCapability: Map<String, CapabilityRoute> = routes.associateBy {
        it.capability
    }

    init {
        require(providersById.size == providers.size) { "provider ids must be unique" }
        require(routesByCapability.size == routes.size) { "capability routes must be unique" }
        val unknownProviders = routes
            .flatMap(CapabilityRoute::providerIds)
            .filterNot(providersById::containsKey)
            .distinct()
        require(unknownProviders.isEmpty()) {
            "routes reference unknown providers: ${unknownProviders.joinToString()}"
        }
    }

    /**
     * Builds the exact provider allowlist for one implemented tool without
     * consulting a possibly stale readiness snapshot. The provider-specific
     * handler owns its live probe; the dispatch boundary attests its receipt.
     */
    fun executionPlan(
        request: CapabilityRequest,
        toolProviderIds: List<String>,
    ): ProviderExecutionPlanDecision {
        val route = routesByCapability[request.capability]
            ?: return ProviderExecutionPlanDecision.Invalid(
                request,
                ProviderPlanRejection.CAPABILITY_NOT_ROUTED,
            )
        val distinctProviderIds = toolProviderIds.distinct()
        if (toolProviderIds.isEmpty() || distinctProviderIds.size != toolProviderIds.size) {
            return ProviderExecutionPlanDecision.Invalid(
                request,
                ProviderPlanRejection.PROVIDER_NOT_DECLARED,
            )
        }
        toolProviderIds.forEach { providerId ->
            if (providerId !in providersById) {
                return ProviderExecutionPlanDecision.Invalid(
                    request,
                    ProviderPlanRejection.PROVIDER_NOT_DECLARED,
                    providerId,
                )
            }
            if (providerId !in route.providerIds) {
                return ProviderExecutionPlanDecision.Invalid(
                    request,
                    ProviderPlanRejection.PROVIDER_OUTSIDE_CAPABILITY_ROUTE,
                    providerId,
                )
            }
        }
        val routeOrder = route.providerIds.filter(toolProviderIds::contains)
        if (routeOrder != toolProviderIds) {
            return ProviderExecutionPlanDecision.Invalid(
                request,
                ProviderPlanRejection.PROVIDER_ORDER_MISMATCH,
            )
        }
        return ProviderExecutionPlanDecision.Planned(
            ProviderExecutionPlan(
                request = request,
                providers = toolProviderIds.map(providersById::getValue),
            ),
        )
    }

    fun attestReceipt(
        plan: ProviderExecutionPlan,
        providerId: String,
    ): ProviderReceiptDecision {
        val priorityIndex = plan.providers.indexOfFirst { it.id == providerId }
        return if (priorityIndex >= 0) {
            ProviderReceiptDecision.Accepted(plan.providers[priorityIndex], priorityIndex)
        } else {
            ProviderReceiptDecision.Rejected(providerId)
        }
    }
}

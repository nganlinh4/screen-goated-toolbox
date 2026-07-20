package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityRequest
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.capability.PhoneControlProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderExecutionPlan
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderExecutionPlanDecision
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderReceiptDecision
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderRouter
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.put

internal fun interface PhoneControlHandlerExecutor {
    suspend fun execute(
        handler: PhoneControlHandler,
        job: PhoneControlToolJobContext,
        requestedTool: String,
        arguments: JsonObject,
    ): PhoneControlToolExecution
}

internal fun interface PhoneControlToolDispatchBoundary {
    suspend fun dispatch(
        job: PhoneControlToolJobContext,
        requestedTool: String,
        arguments: JsonObject,
    ): PhoneControlToolExecution
}

internal fun interface PhoneControlToolFailureReporter {
    fun report(requestedTool: String, jobId: String, error: Throwable)
}

internal object AndroidPhoneControlToolFailureReporter : PhoneControlToolFailureReporter {
    override fun report(requestedTool: String, jobId: String, error: Throwable) {
        Log.e(
            TAG,
            "provider_failure tool=$requestedTool job_id=$jobId " +
                "exception=${error.javaClass.name}: ${error.message.orEmpty()}",
        )
    }

    private const val TAG = "SGTPhoneControlTools"
}

internal class PhoneControlToolDispatcher(
    private val executor: PhoneControlHandlerExecutor,
    private val providerRouter: ProviderRouter,
    private val failureReporter: PhoneControlToolFailureReporter =
        AndroidPhoneControlToolFailureReporter,
    private val observationRecovery: ActionableObservationRecovery =
        NoOpActionableObservationRecovery,
) : PhoneControlToolDispatchBoundary {
    constructor(context: Context) : this(
        executor = AndroidPhoneControlHandlerExecutor(context),
        providerRouter = PhoneControlProviderRegistry.router(context),
        failureReporter = AndroidPhoneControlToolFailureReporter,
        observationRecovery = AndroidActionableObservationRecovery(),
    )

    override suspend fun dispatch(
        job: PhoneControlToolJobContext,
        requestedTool: String,
        arguments: JsonObject,
    ): PhoneControlToolExecution {
        val spec = PhoneControlToolRegistry.byName[requestedTool]
            ?: return unavailableToolResponse(
                job = job,
                requestedTool = requestedTool,
                capability = "unknown_tool",
                provider = "unregistered",
                state = CapabilityState.UNSUPPORTED,
            )
        val handler = spec.handler ?: return unavailableToolResponse(
            job = job,
            requestedTool = requestedTool,
            capability = spec.capability,
            provider = spec.providerIds.first(),
            state = spec.unavailableState,
            requiredUserStep = spec.requiredUserStep,
        )
        val request = CapabilityRequest(spec.capability, requestedTool)
        val plan = when (
            val decision = providerRouter.executionPlan(request, spec.providerIds)
        ) {
            is ProviderExecutionPlanDecision.Planned -> decision.plan
            is ProviderExecutionPlanDecision.Invalid -> {
                return providerPlanFailure(job, spec, decision)
            }
        }
        return try {
            validateReceipt(
                spec = spec,
                plan = plan,
                execution = observationRecovery.recover(
                    executor.execute(handler, job, requestedTool, arguments),
                ),
            )
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Throwable) {
            failureReporter.report(requestedTool, job.jobId, error)
            providerFailure(job, spec, handler)
        }
    }

    private fun providerFailure(
        job: PhoneControlToolJobContext,
        spec: PhoneControlToolSpec,
        handler: PhoneControlHandler,
    ): PhoneControlToolExecution {
        val effect = if (handler.mutating) {
            EffectCertainty.MAY_HAVE_OCCURRED
        } else {
            EffectCertainty.PROVEN_NO_EFFECT
        }
        return PhoneControlToolExecution(
            response = toolResponse(
                job = job,
                requestedTool = spec.name,
                capability = spec.capability,
                provider = UNATTRIBUTED_PROVIDER,
                providerState = CapabilityState.DEGRADED,
                code = "provider_failure",
                observationGeneration = 0,
                effect = effect,
                snapshotInvalidated = handler.mutating,
                retryable = true,
                freshObservationRequired = handler.mutating,
                data = buildJsonObject {
                    put(
                        "message",
                        "The provider failed before it could return a typed receipt.",
                    )
                },
            ),
            mutating = handler.mutating,
            refreshScreenFrame = handler.mutating,
        )
    }

    private fun validateReceipt(
        spec: PhoneControlToolSpec,
        plan: ProviderExecutionPlan,
        execution: PhoneControlToolExecution,
    ): PhoneControlToolExecution {
        val response = execution.response
        val reportedTool = response.stringOrNull("requested_tool")
        val reportedCapability = response.stringOrNull("capability")
        val reportedProvider = response.stringOrNull("provider")
        if (reportedTool != spec.name) {
            return providerReceiptViolation(execution, plan, "requested_tool_mismatch")
        }
        val code = response.stringOrNull("code")
        val effectStatus = response.stringOrNull("effect_status")
        if (reportedCapability == LOCAL_CONTRACT_CAPABILITY) {
            val provenLocalContractFailure = code != null &&
                code != SUCCESS_CODE &&
                !execution.mutating &&
                effectStatus == EffectCertainty.PROVEN_NO_EFFECT.wireName
            return if (provenLocalContractFailure) {
                execution
            } else {
                providerReceiptViolation(execution, plan, "invalid_tool_contract_receipt")
            }
        }
        if (reportedCapability != spec.capability || reportedProvider == null) {
            return providerReceiptViolation(execution, plan, "capability_or_provider_missing")
        }
        val successful = code == SUCCESS_CODE
        val providerRole = response.stringOrNull(PROVIDER_ROLE_FIELD) ?: PRIMARY_PROVIDER_ROLE
        if (providerRole == DEPENDENCY_PROVIDER_ROLE) {
            val validDependencyFailure = !successful &&
                !execution.mutating &&
                effectStatus == EffectCertainty.PROVEN_NO_EFFECT.wireName &&
                reportedProvider in spec.dependencyProviderIds
            return if (validDependencyFailure) {
                execution
            } else {
                providerReceiptViolation(execution, plan, "invalid_dependency_provider_receipt")
            }
        }
        if (providerRole != PRIMARY_PROVIDER_ROLE) {
            return providerReceiptViolation(execution, plan, "invalid_provider_role")
        }
        if (
            successful &&
            response.stringOrNull("provider_state") != CapabilityState.READY.wireName
        ) {
            return providerReceiptViolation(execution, plan, "provider_not_ready")
        }
        return when (providerRouter.attestReceipt(plan, reportedProvider)) {
            is ProviderReceiptDecision.Accepted -> execution
            is ProviderReceiptDecision.Rejected ->
                providerReceiptViolation(execution, plan, "provider_outside_tool_plan")
        }
    }

    private fun providerPlanFailure(
        job: PhoneControlToolJobContext,
        spec: PhoneControlToolSpec,
        decision: ProviderExecutionPlanDecision.Invalid,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = spec.name,
            capability = spec.capability,
            provider = PROVIDER_CONTRACT,
            providerState = CapabilityState.UNSUPPORTED,
            code = PROVIDER_CONTRACT_VIOLATION,
            observationGeneration = 0,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            data = buildJsonObject {
                put("failure_class", INTERNAL_FAILURE_CLASS)
                put("provider_route_error", decision.rejection.name.lowercase())
                decision.providerId?.let { put("reported_provider", it) }
            },
        ),
        mutating = false,
    )

    private fun providerReceiptViolation(
        execution: PhoneControlToolExecution,
        plan: ProviderExecutionPlan,
        reason: String,
    ): PhoneControlToolExecution {
        val response = execution.response.toMutableMap()
        response["code"] = JsonPrimitive(PROVIDER_CONTRACT_VIOLATION)
        response["failure_class"] = JsonPrimitive(INTERNAL_FAILURE_CLASS)
        response["provider_route_error"] = JsonPrimitive(reason)
        response["allowed_providers"] = JsonArray(
            plan.providers.map { JsonPrimitive(it.id) },
        )
        response["retryable"] = JsonPrimitive(false)
        if (execution.mutating) {
            response["snapshot_invalidated"] = JsonPrimitive(true)
            response["fresh_observation_required"] = JsonPrimitive(true)
        }
        return execution.copy(
            response = JsonObject(response),
            refreshScreenFrame = execution.refreshScreenFrame || execution.mutating,
        )
    }

    private fun JsonObject.stringOrNull(name: String): String? =
        (get(name) as? JsonPrimitive)?.contentOrNull

    private companion object {
        const val LOCAL_CONTRACT_CAPABILITY = "tool_contract"
        const val PROVIDER_CONTRACT = "provider_contract"
        const val PROVIDER_CONTRACT_VIOLATION = "provider_contract_failure"
        const val INTERNAL_FAILURE_CLASS = "internal"
        const val SUCCESS_CODE = "ok"
        const val UNATTRIBUTED_PROVIDER = "unattributed_provider_failure"
        const val PROVIDER_ROLE_FIELD = "provider_role"
        const val PRIMARY_PROVIDER_ROLE = "primary"
        const val DEPENDENCY_PROVIDER_ROLE = "dependency"
    }
}

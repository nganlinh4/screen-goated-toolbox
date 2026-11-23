package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.capability.PhoneControlProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import kotlin.coroutines.coroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

internal data class CommandProviderAvailability(
    val state: CapabilityState,
    val guidance: String? = null,
)

internal sealed interface CommandProviderExecution {
    data class Receipt(val data: JsonObject) : CommandProviderExecution

    data class Failure(
        val code: String,
        val message: String,
        val state: CapabilityState,
        val guidance: String? = null,
        val effectMayHaveOccurred: Boolean,
        val requiredUserStep: String? = null,
        val freshObservationRequired: Boolean = false,
    ) : CommandProviderExecution
}

internal interface CommandToolBackend {
    val providerIds: List<String>

    fun probe(providerId: String): CommandProviderAvailability

    suspend fun awaitAvailability(providerId: String): CommandProviderAvailability =
        probe(providerId)

    suspend fun execute(
        job: PhoneControlToolJobContext,
        providerId: String,
        program: String,
        args: List<String>,
        cwd: String,
        timeoutMs: Long,
    ): CommandProviderExecution
}

internal class AndroidCommandToolBackend(context: Context) : CommandToolBackend {
    private val context = context.applicationContext
    private val providersById: Map<String, PrivilegedCommandProvider> =
        PrivilegedCommandProviderRegistry
            .ordered(
                PhoneControlProviderRegistry.providersFor(
                    this.context,
                    COMMAND_CAPABILITY,
                ),
            )
            .associateBy(PrivilegedCommandProvider::providerId)

    override val providerIds: List<String> = providersById.keys.toList()

    override fun probe(providerId: String): CommandProviderAvailability =
        probeSelected(providerId) {
            providersById[providerId]
                ?.probe(context)
                ?.let { CommandProviderAvailability(it.state, it.requiredUserStep) }
                ?: CommandProviderAvailability(
                    CapabilityState.UNSUPPORTED,
                    "No command provider implementation is installed.",
                )
        }

    override suspend fun awaitAvailability(providerId: String): CommandProviderAvailability =
        if (!PhoneControlPowerPreferences.enablesProvider(context, providerId)) {
            probe(providerId)
        } else {
            providersById[providerId]
                ?.awaitReady(context)
                ?.let { CommandProviderAvailability(it.state, it.requiredUserStep) }
                ?: CommandProviderAvailability(
                    CapabilityState.UNSUPPORTED,
                    "No command provider implementation is installed.",
                )
        }

    override suspend fun execute(
        job: PhoneControlToolJobContext,
        providerId: String,
        program: String,
        args: List<String>,
        cwd: String,
        timeoutMs: Long,
    ): CommandProviderExecution {
        if (!PhoneControlPowerPreferences.enablesProvider(context, providerId)) {
            return CommandProviderExecution.Failure(
                code = "provider_not_selected",
                message = "This elevated authority is not selected.",
                state = CapabilityState.UNAVAILABLE,
                guidance = "Select the authority from the Phone Control orb.",
                effectMayHaveOccurred = false,
            )
        }
        val provider = providersById[providerId]
            ?: return CommandProviderExecution.Failure(
                code = "provider_not_implemented",
                message = "The selected authority has no command provider implementation.",
                state = CapabilityState.UNSUPPORTED,
                effectMayHaveOccurred = false,
            )
        val result = provider.executeAuthorized(
            context,
            job.effectOwner,
            program,
            args,
            cwd,
            timeoutMs,
            beforeDispatch = { commandAuthorityPreflight() },
        )
        coroutineContext.ensureActive()
        return result.toCommandExecution()
    }

    private inline fun probeSelected(
        providerId: String,
        probe: () -> CommandProviderAvailability,
    ): CommandProviderAvailability =
        if (PhoneControlPowerPreferences.enablesProvider(context, providerId)) {
            probe()
        } else {
            CommandProviderAvailability(
                CapabilityState.UNAVAILABLE,
                "Select this elevated authority from the Phone Control orb.",
            )
        }

    private fun PrivilegedCommandResult.toCommandExecution(): CommandProviderExecution = when (this) {
        is PrivilegedCommandResult.Success -> CommandProviderExecution.Receipt(receipt)
        is PrivilegedCommandResult.Failure -> CommandProviderExecution.Failure(
            code = code,
            message = message,
            state = state,
            guidance = providerGuidance,
            effectMayHaveOccurred = effectMayHaveOccurred,
            requiredUserStep = requiredUserStep,
            freshObservationRequired = freshObservationRequired,
        )
    }
}

internal suspend fun handleRunCommand(
    job: PhoneControlToolJobContext,
    args: JsonObject,
    backend: CommandToolBackend,
): PhoneControlToolExecution {
    val request = runCatching { parseCommandRequest(args) }.getOrElse {
        return invalidArgs(job, "run_command", it.message ?: "The process request is invalid.")
    }
    val (exactProgram, argv, cwd) = request

    val probes = buildMap {
        backend.providerIds.forEach { providerId ->
            put(providerId, backend.awaitAvailability(providerId))
        }
    }
    val selected = backend.providerIds.firstOrNull { providerId ->
        probes.getValue(providerId).state == CapabilityState.READY
    } ?: return commandUnavailable(job, probes)
    return commandProviderResult(
        job = job,
        providerId = selected,
        result = backend.execute(job, selected, exactProgram, argv, cwd, COMMAND_TIMEOUT_MS),
    )
}

private fun commandProviderResult(
    job: PhoneControlToolJobContext,
    providerId: String,
    result: CommandProviderExecution,
): PhoneControlToolExecution = when (result) {
    is CommandProviderExecution.Receipt -> {
        val receiptCode = result.data["code"]?.jsonPrimitive?.contentOrNull
        val dispatched = receiptCode !in setOf("invalid_request", "launch_failed")
        val effect = if (dispatched) {
            EffectCertainty.MAY_HAVE_OCCURRED
        } else {
            EffectCertainty.PROVEN_NO_EFFECT
        }
        PhoneControlToolExecution(
            response = toolResponse(
                job = job,
                requestedTool = "run_command",
                capability = COMMAND_CAPABILITY,
                provider = providerId,
                providerState = CapabilityState.READY,
                code = if (dispatched) "ok" else receiptCode ?: "command_not_dispatched",
                observationGeneration = 0,
                effect = effect,
                snapshotInvalidated = dispatched,
                freshObservationRequired = dispatched,
                data = buildJsonObject {
                    put("receipt", result.data)
                    val streams = buildJsonObject {
                        for (stream in listOf("output", "stdout", "stderr")) {
                            val text = result.data[stream] as? JsonPrimitive
                            if (text?.isString == true) {
                                val raw = result.data["${stream}_bytes_base64"] as? JsonPrimitive
                                put(stream, buildJsonObject {
                                    put("encoding", "unknown_or_mixed")
                                    put("text_exact", false)
                                    put("raw_bytes_reported_by_provider", raw?.isString == true)
                                })
                            }
                        }
                    }
                    if (streams.isNotEmpty()) put("output_evidence", streams)
                },
            ),
            mutating = dispatched,
            refreshScreenFrame = dispatched,
        )
    }
    is CommandProviderExecution.Failure -> {
        val effect = if (result.effectMayHaveOccurred) {
            EffectCertainty.MAY_HAVE_OCCURRED
        } else {
            EffectCertainty.PROVEN_NO_EFFECT
        }
        PhoneControlToolExecution(
            response = toolResponse(
                job = job,
                requestedTool = "run_command",
                capability = COMMAND_CAPABILITY,
                provider = providerId,
                providerState = result.state,
                code = result.code,
                observationGeneration = 0,
                effect = effect,
                snapshotInvalidated = result.freshObservationRequired ||
                    result.effectMayHaveOccurred,
                retryable = result.state != CapabilityState.UNSUPPORTED,
                requiredUserStep = result.requiredUserStep
                    ?: result.guidance?.let { CONFIGURE_COMMAND_PROVIDER_STEP },
                freshObservationRequired = result.freshObservationRequired ||
                    result.effectMayHaveOccurred,
                data = buildJsonObject {
                    put("message", result.message)
                    result.guidance?.let { put("provider_guidance", it) }
                },
            ),
            mutating = result.effectMayHaveOccurred,
            refreshScreenFrame = result.freshObservationRequired ||
                result.effectMayHaveOccurred,
        )
    }
}

private fun commandUnavailable(
    job: PhoneControlToolJobContext,
    probes: Map<String, CommandProviderAvailability>,
): PhoneControlToolExecution {
    val recoveryProviderId = probes.keys.firstOrNull { providerId ->
        probes.getValue(providerId).state in setOf(
            CapabilityState.DEGRADED,
            CapabilityState.NEEDS_USER_STEP,
            CapabilityState.REVOKED,
        )
    } ?: probes.keys.firstOrNull() ?: NO_COMMAND_PROVIDER
    val recovery = probes[recoveryProviderId] ?: CommandProviderAvailability(
        CapabilityState.UNSUPPORTED,
        "No command provider implementation is installed.",
    )
    return PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = "run_command",
            capability = COMMAND_CAPABILITY,
            provider = recoveryProviderId,
            providerState = recovery.state,
            code = "capability_unavailable",
            observationGeneration = 0,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            retryable = recovery.state != CapabilityState.UNSUPPORTED,
            requiredUserStep = CONFIGURE_COMMAND_PROVIDER_STEP,
            data = buildJsonObject {
                put(
                    "provider_attempts",
                    buildJsonArray {
                        probes.forEach { (providerId, availability) ->
                            add(
                                buildJsonObject {
                                    put("provider", providerId)
                                    put("state", availability.state.wireName)
                                    availability.guidance?.let { put("guidance", it) }
                                },
                            )
                        }
                    },
                )
            },
        ),
        mutating = false,
    )
}

private const val COMMAND_CAPABILITY = "command_execution"
private const val NO_COMMAND_PROVIDER = "command_provider"
private const val CONFIGURE_COMMAND_PROVIDER_STEP = "configure_command_provider"
private const val COMMAND_TIMEOUT_MS = 60_000L

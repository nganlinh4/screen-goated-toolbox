package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.capability.PhoneControlProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import kotlin.coroutines.coroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.serialization.json.JsonArray
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
        val lease = when (val prepared = PhoneControlAccessibilityProvider.prepareCommandDispatch()) {
            is AccessibilityProviderResult.Failure -> return prepared.toCommandFailure()
            is AccessibilityProviderResult.Success -> prepared.value
        }
        val result = provider.executeAuthorized(
            context,
            lease,
            job.effectOwner,
            program,
            args,
            cwd,
            timeoutMs,
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

    private fun AccessibilityProviderResult.Failure.toCommandFailure() =
        CommandProviderExecution.Failure(
            code = code,
            message = message,
            state = if (requiredUserStep != null) {
                CapabilityState.NEEDS_USER_STEP
            } else {
                CapabilityState.DEGRADED
            },
            effectMayHaveOccurred = effect.effectMayHaveOccurred == true,
            requiredUserStep = requiredUserStep
                ?: if (code == "capability_unavailable") "enable_accessibility" else null,
            freshObservationRequired = freshObservationRequired,
        )

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
    val program = args.string("program")
    val command = args.string("command")
    if (program != null && command != null) {
        return invalidArgs(job, "run_command", "supply program or command, not both")
    }
    if (command != null) {
        if (args["args"] != null || args["cwd"] != null) {
            return invalidArgs(job, "run_command", "args and cwd require exact program mode")
        }
        return unavailableToolResponse(
            job,
            "run_command",
            COMMAND_CAPABILITY,
            backend.providerIds.firstOrNull() ?: NO_COMMAND_PROVIDER,
            CapabilityState.UNSUPPORTED,
        )
    }
    val exactProgram = program
        ?.takeIf(String::isNotBlank)
        ?: return invalidArgs(job, "run_command", "run_command requires program")
    if (exactProgram.utf8Size() > MAX_PROGRAM_BYTES || '\u0000' in exactProgram) {
        return invalidArgs(job, "run_command", "program exceeds its bounded exact-argv contract")
    }
    val argv = args.stringList("args")
        ?: return invalidArgs(job, "run_command", "args must be an array of at most $MAX_ARGS strings")
    if (argv.size > MAX_ARGS || argv.any { it.utf8Size() > MAX_ARG_BYTES || '\u0000' in it }) {
        return invalidArgs(job, "run_command", "args exceed the bounded exact-argv contract")
    }
    if (argv.sumOf { it.utf8Size() } > MAX_TOTAL_ARG_BYTES) {
        return invalidArgs(job, "run_command", "process arguments exceed $MAX_TOTAL_ARG_BYTES bytes")
    }
    val cwd = args.string("cwd") ?: DEFAULT_COMMAND_CWD
    if (!cwd.startsWith('/') || cwd.utf8Size() > MAX_CWD_BYTES || '\u0000' in cwd) {
        return invalidArgs(job, "run_command", "cwd must be an absolute bounded directory path")
    }

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
                data = buildJsonObject { put("receipt", result.data) },
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

private fun JsonObject.stringList(name: String): List<String>? {
    val value = get(name) ?: return emptyList()
    val array = value as? JsonArray ?: return null
    if (array.size > MAX_ARGS) return null
    return array.map { element ->
        (element as? JsonPrimitive)?.takeIf(JsonPrimitive::isString)?.contentOrNull ?: return null
    }
}

private fun String.utf8Size(): Int = toByteArray(Charsets.UTF_8).size

private const val COMMAND_CAPABILITY = "command_execution"
private const val NO_COMMAND_PROVIDER = "command_provider"
private const val CONFIGURE_COMMAND_PROVIDER_STEP = "configure_command_provider"
private const val DEFAULT_COMMAND_CWD = "/data/local/tmp"
private const val COMMAND_TIMEOUT_MS = 60_000L
private const val MAX_ARGS = 16
private const val MAX_PROGRAM_BYTES = 1_024
private const val MAX_ARG_BYTES = 4_096
private const val MAX_TOTAL_ARG_BYTES = 16_384
private const val MAX_CWD_BYTES = 4_096

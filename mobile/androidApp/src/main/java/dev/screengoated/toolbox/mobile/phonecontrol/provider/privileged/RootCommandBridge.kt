package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityCommandDispatchLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import java.util.concurrent.atomic.AtomicLong

internal data class RootBridgeProbe(
    val state: CapabilityState,
    val requiredUserStep: String? = null,
)

internal object RootCommandBridge {
    @Volatile
    private var authorized = false

    fun probe(): RootBridgeProbe {
        if (!hasSu()) {
            authorized = false
            return RootBridgeProbe(CapabilityState.UNAVAILABLE, "No root manager is present.")
        }
        return if (authorized) {
            RootBridgeProbe(CapabilityState.READY)
        } else {
            RootBridgeProbe(
                CapabilityState.NEEDS_USER_STEP,
                "Authorize SGT in the root-manager prompt.",
            )
        }
    }

    suspend fun requestAuthorization(): RootBridgeProbe = withContext(Dispatchers.IO) {
        val receipt = defaultBoundedProcessRunner.run(
            operationId = nextInternalOperationId("authorization"),
            command = listOf("su", "-c", "test \"$(id -u)\" = 0"),
            cwd = null,
            timeoutMs = AUTH_TIMEOUT_MS,
            authorityUid = 0,
        )
        authorized = receipt["exit_code"]?.jsonPrimitive?.intOrNull == 0 &&
            receipt["timed_out"]?.jsonPrimitive?.booleanOrNull != true
        probe()
    }

    suspend fun executeAuthorized(
        lease: AccessibilityCommandDispatchLease,
        effectOwner: PhoneControlEffectOwner,
        program: String,
        args: List<String>,
        cwd: String?,
        timeoutMs: Long,
    ): PrivilegedCommandResult = execute(lease, effectOwner, program, args, cwd, timeoutMs)

    suspend fun verifyAuthority(timeoutMs: Long): PrivilegedCommandResult =
        execute(
            lease = null,
            effectOwner = null,
            program = ID_PROGRAM,
            args = listOf(ID_UID_ARGUMENT),
            cwd = null,
            timeoutMs = timeoutMs,
        )

    private suspend fun execute(
        lease: AccessibilityCommandDispatchLease?,
        effectOwner: PhoneControlEffectOwner?,
        program: String,
        args: List<String>,
        cwd: String?,
        timeoutMs: Long,
    ): PrivilegedCommandResult = withContext(Dispatchers.IO) {
        if (!authorized) {
            return@withContext PrivilegedCommandResult.Failure(
                code = "capability_unavailable",
                message = "Root authority is not ready.",
                state = CapabilityState.NEEDS_USER_STEP,
                providerGuidance = "Authorize SGT in the root-manager prompt.",
                effectMayHaveOccurred = false,
            )
        }
        lease?.let { initialLease ->
            PhoneControlAccessibilityProvider.validateCommandDispatch(initialLease)
                ?.let { return@withContext it.toPrivilegedCommandFailure() }
        }
        val finalLease = when (val prepared = PhoneControlAccessibilityProvider.prepareCommandDispatch()) {
            is AccessibilityProviderResult.Failure ->
                return@withContext prepared.toPrivilegedCommandFailure()
            is AccessibilityProviderResult.Success ->
                prepared.value
        }
        PhoneControlAccessibilityProvider.validateCommandDispatch(finalLease)
            ?.let { return@withContext it.toPrivilegedCommandFailure() }
        val invocation = (listOf(program) + args).joinToString(" ", transform = ::shellQuote)
        val guarded = "test \"$(id -u)\" = 0 || exit 126; exec $invocation"
        val operationId = effectOwner?.operationId?.wireValue
            ?: nextInternalOperationId("authority-probe")
        val cancellation = effectOwner?.registerCancellationHandler {
            defaultBoundedProcessRunner.requestCancellation(operationId)
        }
        if (effectOwner != null && cancellation == null) throw CancellationException()
        val effectLease = effectOwner?.beginEffect()
        if (effectOwner != null && effectLease == null) {
            cancellation?.close()
            throw CancellationException()
        }
        val receipt = try {
            defaultBoundedProcessRunner.run(
                operationId = operationId,
                command = listOf("su", "-c", guarded),
                cwd = cwd,
                timeoutMs = timeoutMs,
                authorityUid = 0,
                onProcessStarted = { effectLease?.markAccepted() },
            )
        } catch (error: Throwable) {
            if (error is CancellationException) throw error
            return@withContext PrivilegedCommandResult.Failure(
                code = "root_command_failed",
                message = error.message ?: error.javaClass.simpleName,
                state = CapabilityState.DEGRADED,
                providerGuidance = "Check root-manager authorization and retry.",
                effectMayHaveOccurred = true,
            )
        } finally {
            effectLease?.close()
            cancellation?.close()
        }
        if (receipt["exit_code"]?.jsonPrimitive?.intOrNull == 126) authorized = false
        PrivilegedCommandResult.Success(receipt)
    }

    private fun hasSu(): Boolean = SU_PATHS.any { File(it).exists() }

    private fun shellQuote(value: String): String =
        "'${value.replace("'", "'\\''")}'"

    private fun nextInternalOperationId(kind: String): String =
        "root-$kind-${internalOperations.incrementAndGet()}"

    private val SU_PATHS = listOf("/system/bin/su", "/system/xbin/su", "/sbin/su")
    private const val AUTH_TIMEOUT_MS = 10_000L
    private const val ID_PROGRAM = "/system/bin/id"
    private const val ID_UID_ARGUMENT = "-u"
    private val internalOperations = AtomicLong(0L)
}

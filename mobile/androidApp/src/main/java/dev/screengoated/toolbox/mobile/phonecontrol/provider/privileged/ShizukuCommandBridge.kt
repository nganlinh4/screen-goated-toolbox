package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.ComponentName
import android.content.Context
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.os.Build
import android.os.IBinder
import dev.screengoated.toolbox.mobile.BuildConfig
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityCommandDispatchLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import rikka.shizuku.Shizuku
import java.util.concurrent.atomic.AtomicLong

internal data class ShizukuBridgeProbe(
    val state: CapabilityState,
    val authorityUid: Int? = null,
    val requiredUserStep: String? = null,
)

internal object ShizukuCommandBridge {
    private val lock = Any()
    private val json = Json { ignoreUnknownKeys = true }

    @Volatile
    private var service: IPhoneControlShellService? = null
    private var pendingService: CompletableDeferred<IPhoneControlShellService>? = null

    private val serviceArgs = Shizuku.UserServiceArgs(
        ComponentName(BuildConfig.APPLICATION_ID, PhoneControlShellUserService::class.java.name),
    )
        .daemon(false)
        .processNameSuffix("phone_control_shell")
        .debuggable(BuildConfig.DEBUG)
        .version(BuildConfig.VERSION_CODE)

    private val connection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName, binder: IBinder?) {
            val connected = binder
                ?.takeIf(IBinder::pingBinder)
                ?.let(IPhoneControlShellService.Stub::asInterface)
            synchronized(lock) {
                service = connected
                val pending = pendingService
                pendingService = null
                if (connected != null) {
                    pending?.complete(connected)
                } else {
                    pending?.completeExceptionally(
                        IllegalStateException("Shizuku returned an invalid user-service binder."),
                    )
                }
            }
        }

        override fun onServiceDisconnected(name: ComponentName) {
            synchronized(lock) { service = null }
        }
    }

    fun probe(context: Context): ShizukuBridgeProbe {
        val binderReady = runCatching { Shizuku.pingBinder() }.getOrDefault(false)
        if (!binderReady) {
            return shizukuBinderUnavailable(isShizukuInstalled(context))
        }
        if (runCatching { Shizuku.isPreV11() }.getOrDefault(true)) {
            return ShizukuBridgeProbe(
                CapabilityState.UNSUPPORTED,
                requiredUserStep = "Update Shizuku to API 11 or newer.",
            )
        }
        val permission = runCatching { Shizuku.checkSelfPermission() }.getOrNull()
        if (permission == PackageManager.PERMISSION_GRANTED) {
            return ShizukuBridgeProbe(
                state = CapabilityState.READY,
                authorityUid = runCatching { Shizuku.getUid() }.getOrNull(),
            )
        }
        val deniedPermanently = runCatching {
            Shizuku.shouldShowRequestPermissionRationale()
        }.getOrDefault(false)
        return ShizukuBridgeProbe(
            state = if (deniedPermanently) CapabilityState.REVOKED else CapabilityState.NEEDS_USER_STEP,
            requiredUserStep = if (deniedPermanently) {
                "Allow SGT from Shizuku's authorized-apps screen."
            } else {
                "Grant SGT permission in the Shizuku prompt."
            },
        )
    }

    fun requestPermission(context: Context, requestCode: Int): Boolean {
        val probe = probe(context)
        if (probe.state == CapabilityState.READY) return true
        if (probe.state != CapabilityState.NEEDS_USER_STEP) return false
        return runCatching {
            if (!Shizuku.pingBinder() || Shizuku.shouldShowRequestPermissionRationale()) {
                false
            } else {
                Shizuku.requestPermission(requestCode)
                true
            }
        }.getOrDefault(false)
    }

    suspend fun executeAuthorized(
        context: Context,
        lease: AccessibilityCommandDispatchLease,
        effectOwner: PhoneControlEffectOwner,
        program: String,
        args: List<String>,
        cwd: String?,
        timeoutMs: Long,
    ): PrivilegedCommandResult = execute(
        context,
        lease,
        effectOwner,
        program,
        args,
        cwd,
        timeoutMs,
    )

    suspend fun verifyAuthority(context: Context, timeoutMs: Long): PrivilegedCommandResult =
        execute(
            context = context,
            lease = null,
            effectOwner = null,
            program = ID_PROGRAM,
            args = listOf(ID_UID_ARGUMENT),
            cwd = null,
            timeoutMs = timeoutMs,
        )

    private suspend fun execute(
        context: Context,
        lease: AccessibilityCommandDispatchLease?,
        effectOwner: PhoneControlEffectOwner?,
        program: String,
        args: List<String>,
        cwd: String?,
        timeoutMs: Long,
    ): PrivilegedCommandResult {
        val probe = probe(context)
        if (probe.state != CapabilityState.READY) {
            return PrivilegedCommandResult.Failure(
                code = "capability_unavailable",
                message = "Shizuku shell authority is not ready.",
                state = probe.state,
                providerGuidance = probe.requiredUserStep,
                effectMayHaveOccurred = false,
            )
        }
        var dispatchStarted = false
        return try {
            lease?.let { initialLease ->
                PhoneControlAccessibilityProvider.validateCommandDispatch(initialLease)
                    ?.let { return it.toPrivilegedCommandFailure() }
            }
            val remote = awaitService()
            val operationId = effectOwner?.operationId?.wireValue
                ?: nextInternalOperationId("authority-probe")
            val finalLease = when (val prepared = PhoneControlAccessibilityProvider.prepareCommandDispatch()) {
                is AccessibilityProviderResult.Failure ->
                    return prepared.toPrivilegedCommandFailure()
                is AccessibilityProviderResult.Success ->
                    prepared.value
            }
            val cancellation = effectOwner?.registerCancellationHandler {
                remote.cancelCommand(operationId)
            }
            if (effectOwner != null && cancellation == null) throw CancellationException()
            val effectLease = effectOwner?.beginEffect()
            if (effectOwner != null && effectLease == null) {
                cancellation?.close()
                throw CancellationException()
            }
            try {
                withContext(Dispatchers.IO) {
                    PhoneControlAccessibilityProvider.validateCommandDispatch(finalLease)
                        ?.let { return@withContext it.toPrivilegedCommandFailure() }
                    if (effectLease != null && !effectLease.tryReserveAcceptedDispatch()) {
                        throw CancellationException()
                    }
                    dispatchStarted = true
                    val raw = remote.runCommand(
                        operationId,
                        program,
                        args.toTypedArray(),
                        cwd,
                        timeoutMs,
                    )
                    PrivilegedCommandResult.Success(json.parseToJsonElement(raw).jsonObject)
                }
            } finally {
                effectLease?.close()
                cancellation?.close()
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Throwable) {
            synchronized(lock) { service = null }
            PrivilegedCommandResult.Failure(
                code = "shizuku_command_failed",
                message = error.message ?: error.javaClass.simpleName,
                state = CapabilityState.DEGRADED,
                providerGuidance = "Restart Shizuku and retry after its binder reconnects.",
                effectMayHaveOccurred = dispatchStarted,
            )
        }
    }

    fun close() {
        runCatching { Shizuku.unbindUserService(serviceArgs, connection, true) }
        synchronized(lock) {
            service = null
            pendingService?.cancel()
            pendingService = null
        }
    }

    private suspend fun awaitService(): IPhoneControlShellService {
        service?.takeIf { it.asBinder().pingBinder() }?.let { return it }
        val pending = synchronized(lock) {
            service?.takeIf { it.asBinder().pingBinder() }?.let { return it }
            pendingService ?: CompletableDeferred<IPhoneControlShellService>().also {
                pendingService = it
                try {
                    Shizuku.bindUserService(serviceArgs, connection)
                } catch (error: Throwable) {
                    pendingService = null
                    it.completeExceptionally(error)
                }
            }
        }
        return withTimeout(BIND_TIMEOUT_MS) { pending.await() }
    }

    private fun nextInternalOperationId(kind: String): String =
        "shizuku-$kind-${internalOperations.incrementAndGet()}"

    private const val BIND_TIMEOUT_MS = 10_000L
    private const val ID_PROGRAM = "/system/bin/id"
    private const val ID_UID_ARGUMENT = "-u"
    private const val SHIZUKU_PACKAGE = "moe.shizuku.privileged.api"
    private val internalOperations = AtomicLong(0L)

    @Suppress("DEPRECATION")
    private fun isShizukuInstalled(context: Context): Boolean = runCatching {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.packageManager.getPackageInfo(
                SHIZUKU_PACKAGE,
                PackageManager.PackageInfoFlags.of(0),
            )
        } else {
            context.packageManager.getPackageInfo(SHIZUKU_PACKAGE, 0)
        }
    }.isSuccess
}

internal fun shizukuBinderUnavailable(packageInstalled: Boolean): ShizukuBridgeProbe =
    if (packageInstalled) {
        ShizukuBridgeProbe(
            CapabilityState.NEEDS_USER_STEP,
            requiredUserStep = "Start Shizuku or Sui.",
        )
    } else {
        ShizukuBridgeProbe(
            CapabilityState.UNAVAILABLE,
            requiredUserStep = "Install Shizuku to add shell authority.",
        )
    }

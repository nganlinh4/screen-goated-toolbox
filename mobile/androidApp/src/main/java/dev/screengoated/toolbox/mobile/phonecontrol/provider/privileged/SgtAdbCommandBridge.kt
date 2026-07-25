package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.Context
import android.os.Build
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

internal data class SgtAdbBridgeProbe(
    val state: CapabilityState,
    val condition: SgtAdbBridgeCondition,
    val authorityUid: Int? = null,
    val requiredUserStep: String? = null,
    val pairingEstablished: Boolean = false,
    val deviceIdentity: String? = null,
)

internal enum class SgtAdbBridgeCondition {
    READY,
    API_UNSUPPORTED,
    NOT_PAIRED,
    CONNECTING,
    WIRELESS_DEBUGGING_UNAVAILABLE,
    CONNECTION_ENDPOINT_UNAVAILABLE,
    AUTHORIZATION_REJECTED,
    CONNECTION_FAILED,
    PAIRING_STATE_PERSIST_FAILED,
    AUTHORITY_VERIFICATION_FAILED,
    BRIDGE_UNAVAILABLE,
}

internal object SgtAdbCommandBridge {
    private val lock = Any()
    private val json = Json { ignoreUnknownKeys = true }
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val pairingMutex = Mutex()
    private val cachedProbe = AtomicReference(
        SgtAdbBridgeProbe(
            CapabilityState.NEEDS_USER_STEP,
            SgtAdbBridgeCondition.NOT_PAIRED,
            requiredUserStep = PAIR_GUIDANCE,
        ),
    )

    private var serviceClient: SgtAdbServiceClient? = null
    private var reconnectAttempt: Deferred<SgtAdbBridgeProbe>? = null
    private var reconnectGeneration = 0L

    fun probe(context: Context): SgtAdbBridgeProbe {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            return SgtAdbBridgeProbe(
                CapabilityState.UNSUPPORTED,
                SgtAdbBridgeCondition.API_UNSUPPORTED,
                requiredUserStep = "Android 11 or newer is required for wireless debugging.",
            )
        }
        if (!SgtAdbPairingStore.isPaired(context) || !SgtAdbKeyStore.exists()) {
            return SgtAdbBridgeProbe(
                CapabilityState.NEEDS_USER_STEP,
                SgtAdbBridgeCondition.NOT_PAIRED,
                requiredUserStep = PAIR_GUIDANCE,
            )
        }
        val current = cachedProbe.get()
        if (current.state == CapabilityState.READY) return current
        requestReconnect(context)
        return cachedProbe.get()
    }

    fun hasPairing(context: Context): Boolean =
        SgtAdbPairingStore.isPaired(context) && SgtAdbKeyStore.exists()

    fun requestReconnect(context: Context) {
        reconnectRequest(context.applicationContext)
    }

    suspend fun reconnect(context: Context): SgtAdbBridgeProbe =
        reconnectRequest(context.applicationContext).await()

    suspend fun awaitReady(context: Context): SgtAdbBridgeProbe {
        val current = probe(context)
        if (current.state == CapabilityState.READY || !hasPairing(context)) return current
        return reconnect(context)
    }

    private fun reconnectRequest(context: Context): Deferred<SgtAdbBridgeProbe> {
        val request = synchronized(lock) {
            reconnectAttempt?.takeUnless { it.isCompleted }?.let { return it }
            reconnectGeneration += 1
            val generation = reconnectGeneration
            scope.async(start = CoroutineStart.LAZY) {
                val result = try {
                    parseProbe(awaitRemote(context).connectAndVerify(CONNECT_TIMEOUT_MS))
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (_: Throwable) {
                    BRIDGE_UNAVAILABLE_PROBE
                }
                synchronized(lock) {
                    if (reconnectGeneration == generation) cachedProbe.set(result)
                }
                result
            }.also { pending ->
                reconnectAttempt = pending
                pending.invokeOnCompletion {
                    synchronized(lock) {
                        if (reconnectAttempt === pending) reconnectAttempt = null
                    }
                }
            }
        }
        cachedProbe.set(CONNECTING_PROBE)
        request.start()
        return request
    }

    suspend fun pairProtected(
        context: Context,
        pairingCode: CharArray,
    ): SgtAdbBridgeProbe = pairingMutex.withLock {
        var remote: IPhoneControlAdbService? = null
        var dispatched = false
        try {
            withContext(Dispatchers.IO) {
                val service = awaitRemote(context)
                currentCoroutineContext().ensureActive()
                remote = service
                dispatched = true
                val parsed = parseProbe(
                    service.pairAndVerify(pairingCode.concatToString(), PAIR_TIMEOUT_MS),
                )
                currentCoroutineContext().ensureActive()
                persistPairingProbe(context, parsed).also(cachedProbe::set)
            }
        } catch (cancelled: CancellationException) {
            if (dispatched) abandonPairing(context, remote)
            throw cancelled
        } catch (_: Throwable) {
            if (dispatched) abandonPairing(context, remote)
            BRIDGE_UNAVAILABLE_PROBE.also(cachedProbe::set)
        }
    }

    suspend fun executeAuthorized(
        context: Context,
        effectOwner: PhoneControlEffectOwner,
        program: String,
        args: List<String>,
        cwd: String?,
        timeoutMs: Long,
        effectMayChangeUserState: Boolean = true,
    ): PrivilegedCommandResult {
        val service = try {
            awaitRemote(context)
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Throwable) {
            return bridgeFailure(error, false)
        }
        val liveProbe = try {
            parseProbe(withContext(Dispatchers.IO) {
                service.connectAndVerify(CONNECT_TIMEOUT_MS)
            })
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Throwable) {
            return bridgeFailure(error, false)
        }
        cachedProbe.set(liveProbe)
        if (liveProbe.state != CapabilityState.READY) {
            return PrivilegedCommandResult.Failure(
                code = "capability_unavailable",
                message = "The authenticated local ADB authority is not ready.",
                state = liveProbe.state,
                providerGuidance = liveProbe.requiredUserStep,
                effectMayHaveOccurred = false,
            )
        }
        val operationId = effectOwner.operationId.wireValue
        val cancellation = effectOwner.registerCancellationHandler {
            runCatching { service.cancelCommand(operationId) }
        } ?: throw CancellationException()
        val effectLease = effectOwner.beginEffect()
        if (effectLease == null) {
            cancellation.close()
            throw CancellationException()
        }
        return try {
            if (!effectLease.tryReserveDispatch(effectMayChangeUserState)) {
                throw CancellationException()
            }
            val raw = withContext(Dispatchers.IO) {
                service.runCommand(
                    operationId,
                    program,
                    args.toTypedArray(),
                    cwd,
                    timeoutMs,
                )
            }
            PrivilegedCommandResult.Success(json.parseToJsonElement(raw).jsonObject)
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Throwable) {
            bridgeFailure(error, effectMayChangeUserState)
        } finally {
            effectLease.close()
            cancellation.close()
        }
    }

    suspend fun openBrowserTunnel(context: Context): BrowserTunnelResult =
        withContext(Dispatchers.IO) {
            try {
                val raw = awaitRemote(context).openBrowserTunnel(CONNECT_TIMEOUT_MS)
                val guidance = parseProbe(raw).requiredUserStep ?: RECONNECT_GUIDANCE
                parseBrowserTunnelResult(raw, guidance)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: Throwable) {
                BrowserTunnelResult.Failure(
                    state = CapabilityState.DEGRADED,
                    code = "browser_tunnel_unavailable",
                    requiredUserStep = RECONNECT_GUIDANCE,
                )
            }
        }

    suspend fun closeBrowserTunnel(
        context: Context,
        leaseId: String,
    ): Boolean = withContext(Dispatchers.IO) {
        if (leaseId.isBlank()) return@withContext false
        runCatching {
            val result = json.parseToJsonElement(
                awaitRemote(context).closeBrowserTunnel(leaseId),
            ).jsonObject
            result["ok"]?.jsonPrimitive?.booleanOrNull == true
        }.getOrDefault(false)
    }

    suspend fun forget(context: Context): Boolean = withContext(Dispatchers.IO) {
        retireReconnectAttempt()
        val forgotten = try {
            val result = json.parseToJsonElement(awaitRemote(context).forget()).jsonObject
            result["code"]?.jsonPrimitive?.contentOrNull == "pairing_forgotten"
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: Throwable) {
            false
        }
        if (forgotten) {
            SgtAdbPairingStore.clear(context)
            cachedProbe.set(
                SgtAdbBridgeProbe(
                    CapabilityState.NEEDS_USER_STEP,
                    SgtAdbBridgeCondition.NOT_PAIRED,
                    requiredUserStep = PAIR_GUIDANCE,
                ),
            )
        }
        forgotten
    }

    fun close() {
        retireReconnectAttempt()
        val client = synchronized(lock) {
            serviceClient.also { serviceClient = null }
        }
        client?.close()
    }

    private suspend fun awaitRemote(context: Context): IPhoneControlAdbService =
        serviceClient(context).await()

    private fun serviceClient(context: Context): SgtAdbServiceClient = synchronized(lock) {
        serviceClient ?: SgtAdbServiceClient(
            context = context.applicationContext,
            onUnavailable = ::markBridgeUnavailable,
        ).also { serviceClient = it }
    }

    private fun markBridgeUnavailable() {
        cachedProbe.set(BRIDGE_UNAVAILABLE_PROBE)
    }

    private fun persistPairingProbe(
        context: Context,
        parsed: SgtAdbBridgeProbe,
    ): SgtAdbBridgeProbe {
        if (!parsed.pairingEstablished && parsed.state != CapabilityState.READY) return parsed
        val identity = parsed.deviceIdentity
        if (identity != null && SgtAdbPairingStore.record(context, identity)) return parsed
        return parsed.copy(
            state = CapabilityState.DEGRADED,
            condition = SgtAdbBridgeCondition.PAIRING_STATE_PERSIST_FAILED,
            requiredUserStep = REPAIR_GUIDANCE,
            deviceIdentity = null,
        )
    }

    private suspend fun abandonPairing(
        context: Context,
        remote: IPhoneControlAdbService?,
    ) {
        withContext(NonCancellable + Dispatchers.IO) {
            val forgotten = runCatching {
                val raw = requireNotNull(remote).forget()
                json.parseToJsonElement(raw).jsonObject["code"]
                    ?.jsonPrimitive
                    ?.contentOrNull == "pairing_forgotten"
            }.getOrDefault(false)
            runCatching { SgtAdbPairingStore.clear(context) }
            if (!forgotten) {
                runCatching(SgtAdbKeyStore::delete)
                close()
            }
            cachedProbe.set(NOT_PAIRED_PROBE)
        }
    }

    private fun retireReconnectAttempt() {
        val pending = synchronized(lock) {
            reconnectGeneration += 1
            reconnectAttempt.also { reconnectAttempt = null }
        }
        pending?.cancel()
    }

    internal fun parseProbe(raw: String): SgtAdbBridgeProbe {
        val data = json.parseToJsonElement(raw).jsonObject
        val state = data["state"]?.jsonPrimitive?.contentOrNull
            ?.let { wire -> CapabilityState.entries.firstOrNull { it.wireName == wire } }
            ?: CapabilityState.DEGRADED
        val code = data["code"]?.jsonPrimitive?.contentOrNull.orEmpty()
        val condition = when (code) {
            "ready" -> SgtAdbBridgeCondition.READY
            "wireless_debugging_unavailable",
            "pairing_endpoint_unavailable",
            "pairing_failed",
            "pairing_code_invalid",
            ->
                SgtAdbBridgeCondition.WIRELESS_DEBUGGING_UNAVAILABLE
            "connection_endpoint_unavailable" ->
                SgtAdbBridgeCondition.CONNECTION_ENDPOINT_UNAVAILABLE
            "pairing_authorization_rejected" ->
                SgtAdbBridgeCondition.AUTHORIZATION_REJECTED
            "connection_failed" -> SgtAdbBridgeCondition.CONNECTION_FAILED
            "pairing_state_missing" -> SgtAdbBridgeCondition.NOT_PAIRED
            "paired_connect_pending" -> SgtAdbBridgeCondition.CONNECTING
            "pairing_state_persist_failed" ->
                SgtAdbBridgeCondition.PAIRING_STATE_PERSIST_FAILED
            "authority_verification_failed" ->
                SgtAdbBridgeCondition.AUTHORITY_VERIFICATION_FAILED
            else -> SgtAdbBridgeCondition.BRIDGE_UNAVAILABLE
        }
        val uid = data["authority_uid"]?.jsonPrimitive?.contentOrNull?.toIntOrNull()
        val pairingEstablished =
            data["pairing_established"]?.jsonPrimitive?.booleanOrNull == true
        val deviceIdentity = data["device_identity"]
            ?.jsonPrimitive
            ?.contentOrNull
            ?.takeIf(::isSgtAdbDeviceIdentity)
        val guidance = when (condition) {
            SgtAdbBridgeCondition.READY -> null
            SgtAdbBridgeCondition.PAIRING_STATE_PERSIST_FAILED,
            SgtAdbBridgeCondition.AUTHORITY_VERIFICATION_FAILED,
            SgtAdbBridgeCondition.AUTHORIZATION_REJECTED,
            ->
                REPAIR_GUIDANCE
            SgtAdbBridgeCondition.CONNECTION_ENDPOINT_UNAVAILABLE,
            SgtAdbBridgeCondition.CONNECTION_FAILED,
            SgtAdbBridgeCondition.CONNECTING,
            SgtAdbBridgeCondition.BRIDGE_UNAVAILABLE,
            ->
                RECONNECT_GUIDANCE
            else -> PAIR_GUIDANCE
        }
        return SgtAdbBridgeProbe(
            state,
            condition,
            uid,
            guidance,
            pairingEstablished,
            deviceIdentity,
        )
    }

    private fun bridgeFailure(
        error: Throwable,
        effectMayHaveOccurred: Boolean,
    ) = PrivilegedCommandResult.Failure(
        code = "sgt_adb_command_failed",
        message = error.message ?: error.javaClass.simpleName,
        state = CapabilityState.DEGRADED,
        providerGuidance = RECONNECT_GUIDANCE,
        effectMayHaveOccurred = effectMayHaveOccurred,
        freshObservationRequired = effectMayHaveOccurred,
    )

    private const val PAIR_GUIDANCE =
        "Enable Wireless debugging and open Pair device with pairing code."
    private const val RECONNECT_GUIDANCE =
        "Keep Wireless debugging enabled while SGT reconnects."
    private const val REPAIR_GUIDANCE =
        "Forget the SGT pairing in Wireless debugging, then pair it again."
    private const val CONNECT_TIMEOUT_MS = 8_000L
    private const val PAIR_TIMEOUT_MS = 30_000L

    private val CONNECTING_PROBE = SgtAdbBridgeProbe(
        CapabilityState.DEGRADED,
        SgtAdbBridgeCondition.CONNECTING,
        requiredUserStep = RECONNECT_GUIDANCE,
    )
    private val BRIDGE_UNAVAILABLE_PROBE = SgtAdbBridgeProbe(
        CapabilityState.DEGRADED,
        SgtAdbBridgeCondition.BRIDGE_UNAVAILABLE,
        requiredUserStep = RECONNECT_GUIDANCE,
    )
    private val NOT_PAIRED_PROBE = SgtAdbBridgeProbe(
        CapabilityState.NEEDS_USER_STEP,
        SgtAdbBridgeCondition.NOT_PAIRED,
        requiredUserStep = PAIR_GUIDANCE,
    )
}

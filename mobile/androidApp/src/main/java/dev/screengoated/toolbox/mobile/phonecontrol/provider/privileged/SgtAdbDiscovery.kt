package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import java.net.InetAddress
import java.net.NetworkInterface
import java.util.Collections
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.coroutines.resume
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

internal data class SgtAdbEndpoint(
    val address: InetAddress,
    val port: Int,
    val serviceName: String,
)

internal object SgtAdbDiscovery {
    suspend fun pairing(context: Context, timeoutMs: Long): SgtAdbEndpoint? =
        discover(context, PAIRING_SERVICE_TYPE, expectedServiceName = null, timeoutMs)

    suspend fun connection(
        context: Context,
        expectedServiceName: String,
        timeoutMs: Long,
    ): SgtAdbEndpoint? {
        if (!isSgtAdbDeviceIdentity(expectedServiceName)) return null
        return discover(context, CONNECT_SERVICE_TYPE, expectedServiceName, timeoutMs)
    }

    private suspend fun discover(
        context: Context,
        serviceType: String,
        expectedServiceName: String?,
        timeoutMs: Long,
    ): SgtAdbEndpoint? = withContext(Dispatchers.Main.immediate) {
        withTimeoutOrNull(timeoutMs) {
            suspendCancellableCoroutine { continuation ->
                val manager = context.getSystemService(NsdManager::class.java)
                if (manager == null) {
                    continuation.resume(null)
                    return@suspendCancellableCoroutine
                }
                val session = SgtAdbDiscoverySession(
                    manager = manager,
                    serviceType = serviceType,
                    expectedServiceName = expectedServiceName,
                ) { endpoint ->
                    if (continuation.isActive) continuation.resume(endpoint)
                }
                continuation.invokeOnCancellation { session.stop() }
                session.start()
            }
        }
    }
}

private class SgtAdbDiscoverySession(
    private val manager: NsdManager,
    private val serviceType: String,
    private val expectedServiceName: String?,
    private val complete: (SgtAdbEndpoint?) -> Unit,
) {
    private val lock = Any()
    private val settled = AtomicBoolean(false)
    private val stopRequested = AtomicBoolean(false)
    private val pending = ArrayDeque<NsdServiceInfo>()
    private val queuedNames = mutableSetOf<String>()
    private var resolving = false

    private val discoveryListener = object : NsdManager.DiscoveryListener {
        override fun onDiscoveryStarted(regType: String) = Unit

        override fun onServiceFound(service: NsdServiceInfo) {
            val name = service.serviceName
            if (!sameServiceType(service.serviceType, serviceType) ||
                !matchesSgtAdbServiceIdentity(name, expectedServiceName)
            ) {
                return
            }
            synchronized(lock) {
                if (settled.get() || !queuedNames.add(name)) return
                pending.addLast(service)
            }
            resolveNext()
        }

        override fun onServiceLost(service: NsdServiceInfo) = Unit

        override fun onDiscoveryStopped(serviceType: String) {
            if (!settled.get()) finish(null)
        }

        override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
            finish(null)
        }

        override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) = Unit
    }

    fun start() {
        runCatching {
            manager.discoverServices(
                serviceType,
                NsdManager.PROTOCOL_DNS_SD,
                discoveryListener,
            )
        }.onFailure { finish(null) }
    }

    fun stop() {
        if (!stopRequested.compareAndSet(false, true)) return
        runCatching { manager.stopServiceDiscovery(discoveryListener) }
    }

    @Suppress("DEPRECATION")
    private fun resolveNext() {
        val next = synchronized(lock) {
            if (settled.get() || resolving) return
            pending.removeFirstOrNull()?.also { resolving = true }
        } ?: return
        runCatching {
            manager.resolveService(
                next,
                object : NsdManager.ResolveListener {
                    override fun onResolveFailed(service: NsdServiceInfo, errorCode: Int) {
                        resolutionFinished()
                    }

                    override fun onServiceResolved(service: NsdServiceInfo) {
                        val endpoint = resolvedEndpoint(service)
                        if (endpoint != null) {
                            finish(endpoint)
                        } else {
                            resolutionFinished()
                        }
                    }
                },
            )
        }.onFailure { resolutionFinished() }
    }

    private fun resolutionFinished() {
        synchronized(lock) { resolving = false }
        resolveNext()
    }

    private fun resolvedEndpoint(service: NsdServiceInfo): SgtAdbEndpoint? {
        val name = service.serviceName
        if (!matchesSgtAdbServiceIdentity(name, expectedServiceName) ||
            service.port !in VALID_PORTS
        ) {
            return null
        }
        val address = resolvedAddresses(service).firstOrNull(::isCurrentLocalAddress) ?: return null
        return SgtAdbEndpoint(address, service.port, name)
    }

    private fun finish(endpoint: SgtAdbEndpoint?) {
        if (!settled.compareAndSet(false, true)) return
        stop()
        complete(endpoint)
    }
}

@Suppress("DEPRECATION")
private fun resolvedAddresses(service: NsdServiceInfo): List<InetAddress> =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
        service.hostAddresses
    } else {
        listOfNotNull(service.host)
    }

internal fun matchesSgtAdbServiceIdentity(
    candidate: String,
    expected: String?,
): Boolean {
    if (!isSgtAdbDeviceIdentity(candidate)) return false
    if (expected == null) return true
    if (!isSgtAdbDeviceIdentity(expected)) return false
    return candidate == expected ||
        candidate.startsWith("$expected-") ||
        expected.startsWith("$candidate-") ||
        candidate.substringBeforeLast('-') == expected.substringBeforeLast('-')
}

internal fun isSgtAdbDeviceIdentity(value: String): Boolean =
    value.length in MIN_IDENTITY_CHARS..MAX_IDENTITY_CHARS &&
        value.startsWith(ADB_IDENTITY_PREFIX) &&
        value.last().isLetterOrDigit() &&
        value.all { it.isLetterOrDigit() || it == '-' }

private fun sameServiceType(left: String, right: String): Boolean =
    left.trimEnd('.') == right.trimEnd('.')

private fun isCurrentLocalAddress(candidate: InetAddress): Boolean = runCatching {
    val localAddresses = Collections.list(NetworkInterface.getNetworkInterfaces())
        .flatMap { Collections.list(it.inetAddresses) }
    isLocalEndpointAddress(candidate, localAddresses)
}.getOrDefault(false)

internal fun isLocalEndpointAddress(
    candidate: InetAddress,
    currentInterfaceAddresses: Collection<InetAddress>,
): Boolean = candidate.isLoopbackAddress || currentInterfaceAddresses.any(candidate::equals)

private const val PAIRING_SERVICE_TYPE = "_adb-tls-pairing._tcp"
private const val CONNECT_SERVICE_TYPE = "_adb-tls-connect._tcp"
private const val ADB_IDENTITY_PREFIX = "adb-"
private const val MIN_IDENTITY_CHARS = 5
private const val MAX_IDENTITY_CHARS = 63
private val VALID_PORTS = 1..65_535

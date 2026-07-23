package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.Context
import android.os.Build
import io.github.muntashirakon.adb.AbsAdbConnectionManager
import java.security.PrivateKey
import java.security.cert.Certificate
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

internal class SgtAdbConnectionManager(
    context: Context,
) : AbsAdbConnectionManager() {
    private val appContext = context.applicationContext
    private val key = SgtAdbKeyStore.loadOrCreate()
    private val deviceName = appContext.packageManager
        .getApplicationLabel(appContext.applicationInfo)
        .toString()
        .take(MAX_DEVICE_NAME_CHARS)
        .ifBlank { DEFAULT_DEVICE_NAME }

    init {
        setApi(Build.VERSION.SDK_INT)
        setTimeout(CONNECTION_TIMEOUT_MS, TimeUnit.MILLISECONDS)
        setThrowOnUnauthorised(true)
    }

    override fun getPrivateKey(): PrivateKey = key.privateKey

    override fun getCertificate(): Certificate = key.certificate

    override fun getDeviceName(): String = deviceName

    suspend fun pairAndConnect(pairingCode: String, timeoutMs: Long): SgtAdbPairResult {
        val pairingEndpoint = SgtAdbDiscovery.pairing(appContext, timeoutMs)
            ?: return SgtAdbPairResult(SgtAdbPairStatus.PAIRING_ENDPOINT_UNAVAILABLE)
        val pairingHost = pairingEndpoint.address.hostAddress
            ?: return SgtAdbPairResult(SgtAdbPairStatus.PAIRING_ENDPOINT_UNAVAILABLE)
        val paired = try {
            withContext(Dispatchers.IO) {
                pair(
                    pairingHost,
                    pairingEndpoint.port,
                    pairingCode,
                )
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: Throwable) {
            false
        }
        if (!paired) return SgtAdbPairResult(SgtAdbPairStatus.PAIRING_FAILED)
        if (!SgtAdbPairingStore.record(appContext, pairingEndpoint.serviceName)) {
            return SgtAdbPairResult(
                status = SgtAdbPairStatus.PAIRING_STATE_PERSIST_FAILED,
                pairingEstablished = true,
            )
        }
        return if (connectDiscovered(timeoutMs)) {
            SgtAdbPairResult(
                SgtAdbPairStatus.CONNECTED,
                pairingEstablished = true,
                deviceIdentity = pairingEndpoint.serviceName,
            )
        } else {
            SgtAdbPairResult(
                SgtAdbPairStatus.PAIRED_CONNECT_PENDING,
                pairingEstablished = true,
                deviceIdentity = pairingEndpoint.serviceName,
            )
        }
    }

    suspend fun connectDiscovered(timeoutMs: Long): Boolean {
        if (isConnected) return true
        val deviceIdentity = SgtAdbPairingStore.deviceIdentity(appContext) ?: return false
        val endpoint = SgtAdbDiscovery.connection(
            appContext,
            expectedServiceName = deviceIdentity,
            timeoutMs = timeoutMs,
        ) ?: return false
        val host = endpoint.address.hostAddress ?: return false
        return withContext(Dispatchers.IO) {
            connect(host, endpoint.port) || isConnected
        }
    }

    fun disconnectPreservingKey() {
        runCatching(::disconnect)
    }

    private companion object {
        const val DEFAULT_DEVICE_NAME = "Screen Goated Toolbox"
        const val MAX_DEVICE_NAME_CHARS = 64
        const val CONNECTION_TIMEOUT_MS = 10_000L
    }
}

internal data class SgtAdbPairResult(
    val status: SgtAdbPairStatus,
    val pairingEstablished: Boolean = false,
    val deviceIdentity: String? = null,
)

internal enum class SgtAdbPairStatus {
    PAIRING_ENDPOINT_UNAVAILABLE,
    PAIRING_FAILED,
    PAIRING_STATE_PERSIST_FAILED,
    PAIRED_CONNECT_PENDING,
    CONNECTED,
}

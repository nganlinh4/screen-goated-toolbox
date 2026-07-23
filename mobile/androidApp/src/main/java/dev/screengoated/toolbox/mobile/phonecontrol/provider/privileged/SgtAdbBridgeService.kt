package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.app.Service
import android.content.Intent
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.Process
import android.util.Log
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

internal class SgtAdbBridgeService : Service() {
    private val lock = Any()
    private var manager: SgtAdbConnectionManager? = null
    private var runner: SgtAdbCommandRunner? = null

    private val binder = object : IPhoneControlAdbService.Stub() {
        override fun connectAndVerify(timeoutMs: Long): String =
            runBlocking(Dispatchers.IO) {
                val connected = runCatching {
                    manager().connectDiscovered(timeoutMs)
                }.getOrDefault(false)
                Log.i(TAG, "connect_result connected=$connected")
                if (!connected) {
                    probeResult(
                        state = "needs_user_step",
                        code = "wireless_debugging_unavailable",
                    )
                } else {
                    verifyAuthority(timeoutMs)
                }
            }.toString()

        override fun pairAndVerify(pairingCode: String, timeoutMs: Long): String {
            if (!pairingCode.isAsciiPairingCode()) {
                return probeResult("needs_user_step", "pairing_code_invalid").toString()
            }
            return runBlocking(Dispatchers.IO) {
                val result = runCatching {
                    manager().pairAndConnect(pairingCode, timeoutMs)
                }.getOrDefault(SgtAdbPairResult(SgtAdbPairStatus.PAIRING_FAILED))
                Log.i(
                    TAG,
                    "pair_result result=${result.status.name.lowercase()} " +
                        "pairing_established=${result.pairingEstablished}",
                )
                when (result.status) {
                    SgtAdbPairStatus.PAIRING_ENDPOINT_UNAVAILABLE ->
                        probeResult("needs_user_step", "pairing_endpoint_unavailable")
                    SgtAdbPairStatus.PAIRING_FAILED ->
                        probeResult("needs_user_step", "pairing_failed")
                    SgtAdbPairStatus.PAIRING_STATE_PERSIST_FAILED ->
                        probeResult(
                            "degraded",
                            "pairing_state_persist_failed",
                            pairingEstablished = true,
                        )
                    SgtAdbPairStatus.PAIRED_CONNECT_PENDING ->
                        probeResult(
                            "degraded",
                            "paired_connect_pending",
                            pairingEstablished = true,
                            deviceIdentity = result.deviceIdentity,
                        )
                    SgtAdbPairStatus.CONNECTED ->
                        verifyAuthority(
                            timeoutMs,
                            pairingEstablished = true,
                            deviceIdentity = result.deviceIdentity,
                        )
                }
            }.toString()
        }

        override fun runCommand(
            operationId: String,
            program: String,
            args: Array<out String>,
            cwd: String?,
            timeoutMs: Long,
        ): String = runner().run(operationId, program, args.toList(), cwd, timeoutMs).toString()

        override fun cancelCommand(operationId: String): String =
            runner().cancel(operationId).toString()

        override fun forget(): String {
            synchronized(lock) {
                manager?.disconnectPreservingKey()
                runner = null
                manager = null
            }
            val deleted = runCatching {
                SgtAdbKeyStore.delete()
                true
            }.getOrDefault(false)
            SgtAdbPairingStore.clear(this@SgtAdbBridgeService)
            Log.i(TAG, "forget_result deleted=$deleted")
            if (deleted) {
                Handler(Looper.getMainLooper()).postDelayed(
                    { Process.killProcess(Process.myPid()) },
                    PROCESS_RESET_DELAY_MS,
                )
            }
            return buildJsonObject {
                put("ok", deleted)
                put("code", if (deleted) "pairing_forgotten" else "forget_failed")
            }.toString()
        }
    }

    override fun onBind(intent: Intent?): IBinder = binder

    override fun onDestroy() {
        synchronized(lock) {
            manager?.disconnectPreservingKey()
            runner = null
            manager = null
        }
        super.onDestroy()
    }

    private fun manager(): SgtAdbConnectionManager = synchronized(lock) {
        manager ?: SgtAdbConnectionManager(applicationContext).also {
            manager = it
            runner = SgtAdbCommandRunner(it)
        }
    }

    private fun runner(): SgtAdbCommandRunner = synchronized(lock) {
        runner ?: SgtAdbCommandRunner(manager()).also { runner = it }
    }

    private fun verifyAuthority(
        timeoutMs: Long,
        pairingEstablished: Boolean = false,
        deviceIdentity: String? = SgtAdbPairingStore.deviceIdentity(this),
    ): JsonObject {
        val receipt = runner().run(
            operationId = "adb-authority-${UUID.randomUUID()}",
            program = ID_PROGRAM,
            args = listOf(ID_UID_ARGUMENT),
            cwd = DEFAULT_CWD,
            timeoutMs = timeoutMs.coerceIn(MIN_VERIFY_TIMEOUT_MS, MAX_VERIFY_TIMEOUT_MS),
        )
        val exitCode = receipt["exit_code"]?.jsonPrimitive?.intOrNull
        val uid = receipt["output"]?.jsonPrimitive?.content?.trim()?.toIntOrNull()
        val verified = exitCode == 0 && uid == ADB_SHELL_UID
        Log.i(TAG, "authority_result verified=$verified uid=${uid ?: "unknown"}")
        return if (verified) {
            probeResult(
                "ready",
                "ready",
                ADB_SHELL_UID,
                pairingEstablished = pairingEstablished,
                deviceIdentity = deviceIdentity,
            )
        } else {
            manager().disconnectPreservingKey()
            probeResult(
                "degraded",
                "authority_verification_failed",
                uid,
                pairingEstablished = pairingEstablished,
                deviceIdentity = deviceIdentity,
            )
        }
    }

    private fun probeResult(
        state: String,
        code: String,
        authorityUid: Int? = null,
        pairingEstablished: Boolean = false,
        deviceIdentity: String? = null,
    ): JsonObject = buildJsonObject {
        put("state", state)
        put("code", code)
        put("pairing_established", pairingEstablished)
        authorityUid?.let { put("authority_uid", it) }
        deviceIdentity?.takeIf(::isSgtAdbDeviceIdentity)?.let {
            put("device_identity", it)
        }
    }

    private fun String.isAsciiPairingCode(): Boolean =
        length == PAIRING_CODE_LENGTH && all { it in '0'..'9' }

    private companion object {
        const val ADB_SHELL_UID = 2000
        const val PAIRING_CODE_LENGTH = 6
        const val ID_PROGRAM = "/system/bin/id"
        const val ID_UID_ARGUMENT = "-u"
        const val DEFAULT_CWD = "/data/local/tmp"
        const val MIN_VERIFY_TIMEOUT_MS = 1_000L
        const val MAX_VERIFY_TIMEOUT_MS = 30_000L
        const val PROCESS_RESET_DELAY_MS = 150L
        const val TAG = "SGTPhoneControlAdb"
    }
}

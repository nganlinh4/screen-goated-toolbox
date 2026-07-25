package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.Context
import android.system.Os
import androidx.annotation.Keep
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

@Keep
class PhoneControlShellUserService() : IPhoneControlShellService.Stub() {
    private val lock = Any()
    private var browserTunnel: ShizukuBrowserTunnel? = null

    @Keep
    constructor(@Suppress("UNUSED_PARAMETER") context: Context) : this()

    override fun destroy() {
        synchronized(lock) {
            browserTunnel?.close()
            browserTunnel = null
        }
        kotlin.system.exitProcess(0)
    }

    override fun runCommand(
        operationId: String,
        program: String,
        args: Array<out String>,
        cwd: String?,
        timeoutMs: Long,
    ): String = defaultBoundedProcessRunner.run(
        operationId = operationId,
        command = listOf(program) + args,
        cwd = cwd,
        timeoutMs = timeoutMs,
        authorityUid = Os.getuid(),
    ).toString()

    override fun cancelCommand(operationId: String): String =
        defaultBoundedProcessRunner.requestCancellation(operationId).toString()

    override fun openBrowserTunnel(): String {
        val lease = synchronized(lock) {
            browserTunnel?.takeIf(ShizukuBrowserTunnel::isOpen)
                ?: ShizukuBrowserTunnel().also { browserTunnel = it }
        }.lease
        return buildJsonObject {
            put("state", "ready")
            put("code", "ready")
            put("lease_id", lease.leaseId)
            put("port", lease.port)
            put("bearer_token", lease.bearerToken)
        }.toString()
    }

    override fun closeBrowserTunnel(leaseId: String): String {
        val closed = synchronized(lock) {
            val current = browserTunnel
            if (current?.lease?.leaseId != leaseId) {
                false
            } else {
                current.close()
                browserTunnel = null
                true
            }
        }
        return buildJsonObject {
            put("ok", closed)
            put("code", if (closed) "browser_tunnel_closed" else "browser_tunnel_not_owned")
        }.toString()
    }
}

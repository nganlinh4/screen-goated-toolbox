package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.Context
import android.system.Os
import androidx.annotation.Keep

@Keep
class PhoneControlShellUserService() : IPhoneControlShellService.Stub() {
    @Keep
    constructor(@Suppress("UNUSED_PARAMETER") context: Context) : this()

    override fun destroy() {
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
}

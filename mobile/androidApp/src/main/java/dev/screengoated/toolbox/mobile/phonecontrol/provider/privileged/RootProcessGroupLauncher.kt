package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.io.File

internal const val SETSID_PROGRAM = "/system/bin/setsid"
internal const val SHELL_PROGRAM = "/system/bin/sh"
internal const val OWNED_PROCESS_GROUP_NAME = "sgt-phone-control"

/** Records the new session's group id in `$1`, then execs the remaining argv. */
internal const val OWNED_PROCESS_GROUP_SCRIPT =
    "printf '%s\\n' \"\$\$\" > \"\$1\" || exit 125; shift; exec \"\$@\""

/**
 * Root commands own their process group inside the root shell. A setsid
 * wrapper around the local `su` client would leave the daemon-spawned root
 * shell and its children outside the group that timeout and cancel signal.
 */
internal val rootBoundedProcessRunner =
    BoundedProcessRunner(launcher = BoundedProcessLauncher(::launchRootOwnedProcessGroup))

private fun launchRootOwnedProcessGroup(command: List<String>, cwd: String?): BoundedLaunchedProcess {
    val groupFile = File.createTempFile("sgt-phone-control-root-", ".pgrp")
    return try {
        val process = ProcessBuilder(
            "su",
            "-c",
            rootOwnedProcessGroupScript(groupFile.absolutePath, command),
        )
            .directory(cwd?.let(::File))
            .redirectErrorStream(true)
            .start()
        BoundedLaunchedProcess(process, groupFile)
    } catch (error: Throwable) {
        runCatching { groupFile.delete() }
        throw error
    }
}

/**
 * The background job forks before setsid runs, so setsid never starts from a
 * group leader (where it would fork again and return at once) and the recorded
 * `$$` is the new root group. `wait` keeps the su session open until the
 * command ends and returns its exit status.
 */
internal fun rootOwnedProcessGroupScript(groupFilePath: String, command: List<String>): String {
    val argv = listOf(
        SETSID_PROGRAM,
        SHELL_PROGRAM,
        "-c",
        OWNED_PROCESS_GROUP_SCRIPT,
        OWNED_PROCESS_GROUP_NAME,
        groupFilePath,
    ) + command
    return argv.joinToString(" ", transform = ::rootShellQuote) + " </dev/null & wait \$!"
}

internal fun rootShellQuote(value: String): String =
    "'${value.replace("'", "'\\''")}'"

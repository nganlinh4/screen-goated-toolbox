package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.io.File
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test

class RootProcessGroupLauncherTest {
    @Test
    fun theGroupWrapperRunsInsideTheRootShell() {
        val script = rootOwnedProcessGroupScript(
            "/data/user/0/app/cache/group's.pgrp",
            listOf(SHELL_PROGRAM, "-c", "exec 'find' /"),
        )
        assertTrue(script.startsWith("'/system/bin/setsid' '/system/bin/sh' '-c' "))
        assertTrue(script.contains("'/data/user/0/app/cache/group'\\''s.pgrp'"))
        assertTrue(script.contains("'exec '\\''find'\\'' /'"))
        assertTrue(script.endsWith(" </dev/null & wait \$!"))
    }

    @Test
    fun recordedGroupIsTheCommandsOwnGroupAndExitStatusPropagates() {
        val setsid = listOf("/usr/bin/setsid", "/bin/setsid").map(::File).firstOrNull(File::canExecute)
        assumeTrue(setsid != null && File("/bin/sh").canExecute() && File("/proc/self/stat").exists())
        val groupFile = File.createTempFile("sgt-root-group-test-", ".pgrp")
        try {
            val script = rootOwnedProcessGroupScript(
                groupFile.absolutePath,
                listOf("/bin/sh", "-c", "cut -d' ' -f5 /proc/\$\$/stat; exit 7"),
            )
                .replace("'$SETSID_PROGRAM'", "'${requireNotNull(setsid).absolutePath}'")
                .replace("'$SHELL_PROGRAM'", "'/bin/sh'")
            val process = ProcessBuilder("/bin/sh", "-c", script).redirectErrorStream(true).start()
            assertTrue(process.waitFor(10, TimeUnit.SECONDS))
            val reportedGroup = process.inputStream.bufferedReader().readText().trim()
            assertEquals(7, process.exitValue())
            assertEquals(groupFile.readText().trim(), reportedGroup)
        } finally {
            groupFile.delete()
        }
    }

    @Test
    fun rootCommandsAndTheirCancellationUseTheRootOwnedRunner() {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        val bridge = File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/" +
                "provider/privileged/RootCommandBridge.kt",
        ).readText()
        val execute = bridge.substringAfter("private suspend fun execute(")
        assertTrue(execute.contains("rootBoundedProcessRunner.requestCancellation(operationId)"))
        assertTrue(execute.contains("rootBoundedProcessRunner.run("))
        assertTrue(execute.contains("command = listOf(SHELL_PROGRAM, \"-c\", guarded)"))
        assertTrue(!execute.contains("defaultBoundedProcessRunner"))
    }

    @Test
    fun onlyARootGroupSignalledFromANonRootProcessUsesTheSuClient() {
        val appUid = 10_123
        assertTrue(processGroupNeedsRootClient(authorityUid = 0, callerUid = appUid))
        assertFalse("a root process signals its own root group directly", processGroupNeedsRootClient(0, 0))
        assertFalse(processGroupNeedsRootClient(authorityUid = appUid, callerUid = appUid))
        assertFalse(processGroupNeedsRootClient(authorityUid = 2_000, callerUid = 2_000))
    }

    @Test
    fun theRootAuthorizationProbeOwnsAnAppUidGroup() {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile)
        }
        val bridge = File(
            directory,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/" +
                "provider/privileged/RootCommandBridge.kt",
        ).readText()
        val probe = bridge.substringAfter("suspend fun requestAuthorization()")
            .substringBefore("authorized = ")
        assertTrue(probe.contains("defaultBoundedProcessRunner.run("))
        assertTrue(probe.contains("authorityUid = android.os.Process.myUid()"))
        assertFalse(probe.contains("authorityUid = 0"))
    }
}

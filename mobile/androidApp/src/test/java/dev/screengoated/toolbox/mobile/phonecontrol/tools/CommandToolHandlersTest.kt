package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.add
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CommandToolHandlersTest {
    @Test
    fun exactArgvUsesSgtBridgeBeforeShizukuAndRootAndReturnsUnverifiedEffectReceipt() = runTest {
        val backend = FakeCommandBackend(
            sgt = CommandProviderAvailability(CapabilityState.READY),
            shizuku = CommandProviderAvailability(CapabilityState.READY),
            root = CommandProviderAvailability(CapabilityState.READY),
            sgtResult = processReceipt(exitCode = 0),
        )
        val args = buildJsonObject {
            put("program", "/system/bin/id")
            put("args", buildJsonArray { add("-u") })
        }

        val execution = handleRunCommand(JOB, args, backend)

        assertEquals(1, backend.calls.size)
        assertEquals(JOB.operationId, backend.calls.single().operationId)
        assertEquals(SGT_PROVIDER, backend.calls.single().providerId)
        assertEquals(listOf("-u"), backend.calls.single().args)
        assertEquals("/data/local/tmp", backend.calls.single().cwd)
        assertEquals("ok", execution.response.stringValue("code"))
        assertEquals("sgt_adb_bridge", execution.response.stringValue("provider"))
        assertEquals("may_have_occurred", execution.response.stringValue("effect_status"))
        assertEquals(
            "process_exited",
            execution.response.getValue("receipt").jsonObject.stringValue("code"),
        )
        assertTrue(execution.mutating)
        assertTrue(execution.refreshScreenFrame)
    }

    @Test
    fun rootIsUsedOnlyWhenShizukuIsNotReady() = runTest {
        val backend = FakeCommandBackend(
            shizuku = CommandProviderAvailability(
                CapabilityState.NEEDS_USER_STEP,
                "Start Shizuku.",
            ),
            root = CommandProviderAvailability(CapabilityState.READY),
            rootResult = processReceipt(exitCode = 7),
        )
        val args = buildJsonObject {
            put("program", "/system/bin/false")
            put("cwd", "/data/local/tmp")
        }

        val execution = handleRunCommand(JOB, args, backend)

        assertEquals(ROOT_PROVIDER, backend.calls.single().providerId)
        assertEquals("root_bridge", execution.response.stringValue("provider"))
        assertEquals("ok", execution.response.stringValue("code"))
        assertEquals(
            7,
            execution.response.getValue("receipt").jsonObject
                .getValue("exit_code").jsonPrimitive.content.toInt(),
        )
        assertEquals("may_have_occurred", execution.response.stringValue("effect_status"))
    }

    @Test
    fun selectedProviderColdReconnectIsAwaitedBeforeAvailabilityIsDecided() = runTest {
        val backend = FakeCommandBackend(
            shizuku = CommandProviderAvailability(
                CapabilityState.DEGRADED,
                "Reconnect is in progress.",
            ),
            root = CommandProviderAvailability(CapabilityState.UNAVAILABLE),
            awaitedShizuku = CommandProviderAvailability(CapabilityState.READY),
        )

        val execution = handleRunCommand(
            JOB,
            buildJsonObject { put("program", "/system/bin/id") },
            backend,
        )

        assertEquals(listOf(SGT_PROVIDER, SHIZUKU_PROVIDER, ROOT_PROVIDER), backend.awaits)
        assertEquals(SHIZUKU_PROVIDER, backend.calls.single().providerId)
        assertEquals("ok", execution.response.stringValue("code"))
    }

    @Test
    fun unavailableAuthoritiesKeepToolNameAndExposeBothAttempts() = runTest {
        val backend = FakeCommandBackend(
            shizuku = CommandProviderAvailability(
                CapabilityState.NEEDS_USER_STEP,
                "Start Shizuku.",
            ),
            root = CommandProviderAvailability(
                CapabilityState.UNAVAILABLE,
                "No root manager.",
            ),
        )

        val execution = handleRunCommand(
            JOB,
            buildJsonObject { put("program", "/system/bin/id") },
            backend,
        )
        val attempts = execution.response.getValue("provider_attempts").jsonArray

        assertTrue(backend.calls.isEmpty())
        assertEquals("capability_unavailable", execution.response.stringValue("code"))
        assertEquals("run_command", execution.response.stringValue("requested_tool"))
        assertEquals("shizuku_shell", execution.response.stringValue("provider"))
        assertEquals("needs_user_step", execution.response.stringValue("provider_state"))
        assertEquals(
            "configure_command_provider",
            execution.response.getValue("required_user_step").jsonObject.stringValue("code"),
        )
        assertEquals(listOf("sgt_adb_bridge", "shizuku_shell", "root_bridge"), attempts.map {
            it.jsonObject.stringValue("provider")
        })
        assertEquals("proven_no_effect", execution.response.stringValue("effect_status"))
        assertFalse(execution.mutating)
    }

    @Test
    fun providerFailureAfterPossibleDispatchIsNeverReportedAsNoEffect() = runTest {
        val backend = FakeCommandBackend(
            shizuku = CommandProviderAvailability(CapabilityState.READY),
            root = CommandProviderAvailability(CapabilityState.READY),
            shizukuResult = CommandProviderExecution.Failure(
                code = "shizuku_command_failed",
                message = "binder disconnected",
                state = CapabilityState.DEGRADED,
                guidance = "Restart Shizuku.",
                effectMayHaveOccurred = true,
            ),
        )

        val execution = handleRunCommand(
            JOB,
            buildJsonObject { put("program", "/system/bin/setprop") },
            backend,
        )

        assertEquals("shizuku_command_failed", execution.response.stringValue("code"))
        assertEquals("degraded", execution.response.stringValue("provider_state"))
        assertFalse(execution.response.containsKey("provider_post_state"))
        assertEquals("may_have_occurred", execution.response.stringValue("effect_status"))
        assertTrue(execution.response.getValue("snapshot_invalidated").jsonPrimitive.content.toBoolean())
        assertTrue(execution.mutating)
    }

    @Test
    fun structuralAuthorityRefusalCannotDispatchAndPreservesRequiredUserStep() = runTest {
        val backend = FakeCommandBackend(
            shizuku = CommandProviderAvailability(CapabilityState.READY),
            root = CommandProviderAvailability(CapabilityState.READY),
            shizukuResult = CommandProviderExecution.Failure(
                code = "os_owned_confirmation",
                message = "complete the Android-owned step",
                state = CapabilityState.NEEDS_USER_STEP,
                effectMayHaveOccurred = false,
                requiredUserStep = "complete_os_owned_confirmation",
            ),
        )

        val execution = handleRunCommand(
            JOB,
            buildJsonObject { put("program", "/system/bin/example") },
            backend,
        )

        assertEquals("os_owned_confirmation", execution.response.stringValue("code"))
        assertEquals("proven_no_effect", execution.response.stringValue("effect_status"))
        assertEquals(
            "complete_os_owned_confirmation",
            execution.response.getValue("required_user_step").jsonObject.stringValue("code"),
        )
        assertFalse(execution.mutating)
        assertFalse(execution.refreshScreenFrame)
    }

    @Test
    fun commandStringUsesBoundedPlatformShellFallback() = runTest {
        val backend = FakeCommandBackend(
            sgt = CommandProviderAvailability(CapabilityState.READY),
            shizuku = CommandProviderAvailability(CapabilityState.READY),
            root = CommandProviderAvailability(CapabilityState.READY),
        )

        val execution = handleRunCommand(
            JOB,
            buildJsonObject { put("command", "echo must-not-run") },
            backend,
        )

        val call = backend.calls.single()
        assertEquals("/system/bin/sh", call.program)
        assertEquals(listOf("-c", "echo must-not-run"), call.args)
        assertEquals("ok", execution.response.stringValue("code"))
        assertEquals("may_have_occurred", execution.response.stringValue("effect_status"))
    }

    @Test
    fun shellFallbackRejectsAnArgumentVectorBeyondItsExactByteBudget() = runTest {
        val backend = FakeCommandBackend(
            sgt = CommandProviderAvailability(CapabilityState.READY),
            shizuku = CommandProviderAvailability(CapabilityState.READY),
            root = CommandProviderAvailability(CapabilityState.READY),
        )

        val execution = handleRunCommand(
            JOB,
            buildJsonObject { put("command", "x".repeat(16_383)) },
            backend,
        )

        assertTrue(backend.calls.isEmpty())
        assertEquals("invalid_arguments", execution.response.stringValue("code"))
        assertEquals("proven_no_effect", execution.response.stringValue("effect_status"))
    }

    private data class CommandCall(
        val operationId: String,
        val providerId: String,
        val program: String,
        val args: List<String>,
        val cwd: String,
        val timeoutMs: Long,
    )

    private class FakeCommandBackend(
        private val sgt: CommandProviderAvailability =
            CommandProviderAvailability(CapabilityState.UNAVAILABLE),
        private val shizuku: CommandProviderAvailability,
        private val root: CommandProviderAvailability,
        private val sgtResult: CommandProviderExecution = processReceipt(0),
        private val shizukuResult: CommandProviderExecution = processReceipt(0),
        private val rootResult: CommandProviderExecution = processReceipt(0),
        private val awaitedShizuku: CommandProviderAvailability? = null,
    ) : CommandToolBackend {
        override val providerIds = listOf(SGT_PROVIDER, SHIZUKU_PROVIDER, ROOT_PROVIDER)
        val probes = mutableListOf<String>()
        val awaits = mutableListOf<String>()
        val calls = mutableListOf<CommandCall>()

        override fun probe(providerId: String): CommandProviderAvailability {
            probes += providerId
            return when (providerId) {
                SGT_PROVIDER -> sgt
                SHIZUKU_PROVIDER -> shizuku
                else -> root
            }
        }

        override suspend fun awaitAvailability(
            providerId: String,
        ): CommandProviderAvailability {
            awaits += providerId
            return when (providerId) {
                SGT_PROVIDER -> sgt
                SHIZUKU_PROVIDER -> awaitedShizuku ?: shizuku
                else -> root
            }
        }

        override suspend fun execute(
            job: PhoneControlToolJobContext,
            providerId: String,
            program: String,
            args: List<String>,
            cwd: String,
            timeoutMs: Long,
        ): CommandProviderExecution {
            calls += CommandCall(job.operationId, providerId, program, args, cwd, timeoutMs)
            return when (providerId) {
                SGT_PROVIDER -> sgtResult
                SHIZUKU_PROVIDER -> shizukuResult
                else -> rootResult
            }
        }
    }

    private companion object {
        val JOB = PhoneControlToolJobContext(
            turnId = 5,
            jobId = "job-command-test",
            responseGeneration = 6,
        )
        const val SGT_PROVIDER = "sgt_adb_bridge"
        const val SHIZUKU_PROVIDER = "shizuku_shell"
        const val ROOT_PROVIDER = "root_bridge"

        fun processReceipt(exitCode: Int): CommandProviderExecution =
            CommandProviderExecution.Receipt(
                buildJsonObject {
                    put("ok", true)
                    put("code", "process_exited")
                    put("exit_code", exitCode)
                    put("timed_out", false)
                    put("output", "")
                },
            )

        fun JsonObject.stringValue(name: String): String =
            getValue(name).jsonPrimitive.content
    }
}

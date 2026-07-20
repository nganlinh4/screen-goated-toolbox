package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlAuthorityFixture
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.capability.ProviderRouter
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import java.io.File
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlToolDispatcherTest {
    @Test
    fun everyGeneratedCatalogToolDispatchesExactlyOrReturnsTypedUnavailable() = runTest {
        val generatedNames = generatedCatalogNames()
        assertEquals(62, generatedNames.size)
        assertEquals(generatedNames.size, generatedNames.toSet().size)
        assertEquals(generatedNames, PhoneControlToolRegistry.specs.map { it.name })

        val calls = mutableListOf<Pair<PhoneControlHandler, String>>()
        val dispatcher = PhoneControlToolDispatcher(
            executor = PhoneControlHandlerExecutor { handler, job, requestedTool, _ ->
                calls += handler to requestedTool
                val spec = PhoneControlToolRegistry.byName.getValue(requestedTool)
                PhoneControlToolExecution(
                    response = toolResponse(
                        job = job,
                        requestedTool = requestedTool,
                        capability = spec.capability,
                        provider = spec.providerIds.first(),
                        providerState = CapabilityState.READY,
                        code = DISPATCHED,
                        observationGeneration = 0,
                        effect = EffectCertainty.PROVEN_NO_EFFECT,
                        snapshotInvalidated = false,
                    ),
                    mutating = handler.mutating,
                )
            },
            providerRouter = ROUTER,
            failureReporter = PhoneControlToolFailureReporter { _, _, _ ->
                error("no provider failure expected")
            },
        )

        for (name in generatedNames) {
            val before = calls.size
            val spec = PhoneControlToolRegistry.byName.getValue(name)
            val response = dispatcher.dispatch(JOB, name, JsonObject(emptyMap())).response
            assertEquals(name, response.stringValue("requested_tool"))
            assertEquals(spec.capability, response.stringValue("capability"))
            assertEquals(spec.providerIds.first(), response.stringValue("provider"))
            if (spec.handler == null) {
                assertEquals("capability_unavailable", response.stringValue("code"))
                assertEquals(before, calls.size)
                assertEquals("proven_no_effect", response.stringValue("effect_status"))
            } else {
                assertEquals(DISPATCHED, response.stringValue("code"))
                assertEquals(before + 1, calls.size)
                assertEquals(spec.handler to name, calls.last())
            }
        }
    }

    @Test
    fun unknownNamesAndArgumentsNeverRerouteToAnotherTool() = runTest {
        var dispatchCount = 0
        val dispatcher = PhoneControlToolDispatcher(
            executor = PhoneControlHandlerExecutor { _, _, _, _ ->
                dispatchCount += 1
                error("unsupported tools must not dispatch")
            },
            providerRouter = ROUTER,
            failureReporter = PhoneControlToolFailureReporter { _, _, _ -> },
        )
        val unknown = dispatcher.dispatch(
            JOB,
            "future_unregistered_tool",
            buildJsonObject { put("instruction", "must not reroute") },
        ).response
        val unsupported = dispatcher.dispatch(
            JOB,
            "browser_eval",
            buildJsonObject { put("expression", "document.title") },
        ).response

        assertEquals(0, dispatchCount)
        assertEquals("capability_unavailable", unknown.stringValue("code"))
        assertEquals("future_unregistered_tool", unknown.stringValue("requested_tool"))
        assertEquals("unregistered", unknown.stringValue("provider"))
        assertEquals("capability_unavailable", unsupported.stringValue("code"))
        assertEquals("browser_eval", unsupported.stringValue("requested_tool"))
        assertEquals("browser_cdp", unsupported.stringValue("provider"))
    }

    @Test
    fun providerFailureIsReportedWithoutBeingDiscarded() = runTest {
        val failure = IllegalStateException("provider exploded")
        var reportedTool: String? = null
        var reportedJob: String? = null
        var reportedFailure: Throwable? = null
        val dispatcher = PhoneControlToolDispatcher(
            executor = PhoneControlHandlerExecutor { _, _, _, _ -> throw failure },
            providerRouter = ROUTER,
            failureReporter = PhoneControlToolFailureReporter { tool, jobId, error ->
                reportedTool = tool
                reportedJob = jobId
                reportedFailure = error
            },
        )

        val execution = dispatcher.dispatch(
            JOB,
            "edit_text_file",
            buildJsonObject { put("secret", "must-not-reach-reporter") },
        )

        assertEquals("edit_text_file", reportedTool)
        assertEquals(JOB.jobId, reportedJob)
        assertSame(failure, reportedFailure)
        assertEquals("provider_failure", execution.response.stringValue("code"))
        assertEquals("edit_text_file", execution.response.stringValue("requested_tool"))
        assertEquals(
            "unattributed_provider_failure",
            execution.response.stringValue("provider"),
        )
        assertEquals("may_have_occurred", execution.response.stringValue("effect_status"))
        assertTrue(execution.mutating)
        assertTrue(execution.refreshScreenFrame)
        assertFalse(execution.response.toString().contains("must-not-reach-reporter"))
    }

    @Test
    fun compositeFallbackProviderOnExactToolPlanIsAccepted() = runTest {
        val dispatcher = dispatcherReturning(
            tool = "run_command",
            provider = "root_bridge",
            effect = EffectCertainty.MAY_HAVE_OCCURRED,
        )

        val execution = dispatcher.dispatch(JOB, "run_command", JsonObject(emptyMap()))

        assertEquals("ok", execution.response.stringValue("code"))
        assertEquals("root_bridge", execution.response.stringValue("provider"))
    }

    @Test
    fun providerOnCapabilityRouteButOutsideExactToolPlanIsRejected() = runTest {
        val dispatcher = dispatcherReturning(
            tool = "launch_app",
            provider = "accessibility",
            effect = EffectCertainty.MAY_HAVE_OCCURRED,
        )

        val execution = dispatcher.dispatch(JOB, "launch_app", JsonObject(emptyMap()))

        assertEquals("provider_contract_failure", execution.response.stringValue("code"))
        assertEquals("internal", execution.response.stringValue("failure_class"))
        assertEquals("accessibility", execution.response.stringValue("provider"))
        assertTrue(execution.response.getValue("fresh_observation_required").jsonPrimitive.boolean)
    }

    @Test
    fun provenNoEffectFailureStillAttestsItsPrimaryProvider() = runTest {
        val dispatcher = dispatcherReturning(
            tool = "launch_app",
            provider = "accessibility",
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            code = "provider_failed",
        )

        val execution = dispatcher.dispatch(JOB, "launch_app", JsonObject(emptyMap()))

        assertEquals("provider_contract_failure", execution.response.stringValue("code"))
        assertEquals(
            "provider_outside_tool_plan",
            execution.response.stringValue("provider_route_error"),
        )
    }

    @Test
    fun effectfulFailurePreservesHonestDegradedProviderState() = runTest {
        val dispatcher = dispatcherReturning(
            tool = "launch_app",
            provider = "android_app_api",
            effect = EffectCertainty.MAY_HAVE_OCCURRED,
            providerState = CapabilityState.DEGRADED,
            code = "provider_failed",
        )

        val execution = dispatcher.dispatch(JOB, "launch_app", JsonObject(emptyMap()))

        assertEquals("provider_failed", execution.response.stringValue("code"))
        assertEquals("degraded", execution.response.stringValue("provider_state"))
        assertEquals("may_have_occurred", execution.response.stringValue("effect_status"))
    }

    @Test
    fun successfulReceiptStillRequiresReadyPrimaryProvider() = runTest {
        val dispatcher = dispatcherReturning(
            tool = "launch_app",
            provider = "android_app_api",
            effect = EffectCertainty.MAY_HAVE_OCCURRED,
            providerState = CapabilityState.DEGRADED,
        )

        val execution = dispatcher.dispatch(JOB, "launch_app", JsonObject(emptyMap()))

        assertEquals("provider_contract_failure", execution.response.stringValue("code"))
        assertEquals("provider_not_ready", execution.response.stringValue("provider_route_error"))
    }

    @Test
    fun provenNoEffectToolContractFailureBypassesProviderAttestation() = runTest {
        val dispatcher = dispatcherReturning(
            tool = "launch_app",
            provider = "android_app_api",
            capability = "tool_contract",
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            code = "invalid_arguments",
        )

        val execution = dispatcher.dispatch(JOB, "launch_app", JsonObject(emptyMap()))

        assertEquals("invalid_arguments", execution.response.stringValue("code"))
        assertEquals("tool_contract", execution.response.stringValue("capability"))
    }

    @Test
    fun successfulOrEffectfulToolContractReceiptCannotBypassAttestation() = runTest {
        val cases = listOf(
            Triple("ok", EffectCertainty.PROVEN_NO_EFFECT, false),
            Triple("invalid_arguments", EffectCertainty.MAY_HAVE_OCCURRED, true),
        )
        cases.forEach { (code, effect, mutating) ->
            val dispatcher = dispatcherReturning(
                tool = "launch_app",
                provider = "android_app_api",
                capability = "tool_contract",
                effect = effect,
                code = code,
                mutating = mutating,
            )

            val execution = dispatcher.dispatch(JOB, "launch_app", JsonObject(emptyMap()))

            assertEquals("provider_contract_failure", execution.response.stringValue("code"))
            assertEquals(
                "invalid_tool_contract_receipt",
                execution.response.stringValue("provider_route_error"),
            )
        }
    }

    @Test
    fun malformedReceiptFieldBecomesTypedContractFailureWithoutGenericException() = runTest {
        var genericFailureReported = false
        val dispatcher = PhoneControlToolDispatcher(
            executor = PhoneControlHandlerExecutor { _, job, requestedTool, _ ->
                val response = toolResponse(
                    job = job,
                    requestedTool = requestedTool,
                    capability = "system_query",
                    provider = "android_app_api",
                    providerState = CapabilityState.READY,
                    code = "ok",
                    observationGeneration = 0,
                    effect = EffectCertainty.PROVEN_NO_EFFECT,
                    snapshotInvalidated = false,
                ).toMutableMap()
                response["provider"] = buildJsonObject { put("malformed", true) }
                PhoneControlToolExecution(
                    response = JsonObject(response),
                    mutating = false,
                )
            },
            providerRouter = ROUTER,
            failureReporter = PhoneControlToolFailureReporter { _, _, _ ->
                genericFailureReported = true
            },
        )

        val execution = dispatcher.dispatch(JOB, "system_query", JsonObject(emptyMap()))

        assertEquals("provider_contract_failure", execution.response.stringValue("code"))
        assertEquals(
            "capability_or_provider_missing",
            execution.response.stringValue("provider_route_error"),
        )
        assertFalse(genericFailureReported)
    }

    @Test
    fun dependencyProviderEvidenceDoesNotReplacePrimaryRoute() = runTest {
        val dispatcher = dispatcherReturning(
            tool = "click_target",
            provider = "local_ui_detector",
            effect = EffectCertainty.MAY_HAVE_OCCURRED,
            data = buildJsonObject { put("input_provider", "android_app_api") },
        )

        val execution = dispatcher.dispatch(JOB, "click_target", JsonObject(emptyMap()))

        assertEquals("ok", execution.response.stringValue("code"))
        assertEquals("local_ui_detector", execution.response.stringValue("provider"))
        assertEquals("android_app_api", execution.response.stringValue("input_provider"))
    }

    @Test
    fun provenNoEffectDependencyFailureKeepsItsTruthfulProvider() = runTest {
        val dispatcher = PhoneControlToolDispatcher(
            executor = PhoneControlHandlerExecutor { _, job, requestedTool, _ ->
                PhoneControlToolExecution(
                    response = toolResponse(
                        job = job,
                        requestedTool = requestedTool,
                        capability = "ui.text_edit",
                        provider = "android_app_api",
                        providerState = CapabilityState.READY,
                        code = "artifact_not_found",
                        observationGeneration = 0,
                        effect = EffectCertainty.PROVEN_NO_EFFECT,
                        snapshotInvalidated = false,
                        data = buildJsonObject { put("provider_role", "dependency") },
                    ),
                    mutating = false,
                )
            },
            providerRouter = ROUTER,
            failureReporter = PhoneControlToolFailureReporter { _, _, _ ->
                error("no provider failure expected")
            },
        )

        val execution = dispatcher.dispatch(JOB, "paste_artifact", JsonObject(emptyMap()))

        assertEquals("artifact_not_found", execution.response.stringValue("code"))
        assertEquals("android_app_api", execution.response.stringValue("provider"))
    }

    @Test
    fun dependencyReceiptRequiresExactToolDeclarationAndNoEffect() = runTest {
        val cases = listOf(
            dispatcherReturning(
                tool = "launch_app",
                provider = "accessibility",
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                code = "dependency_failed",
                data = buildJsonObject { put("provider_role", "dependency") },
            ) to "launch_app",
            dispatcherReturning(
                tool = "paste_artifact",
                provider = "android_app_api",
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
                code = "dependency_failed",
                data = buildJsonObject { put("provider_role", "dependency") },
            ) to "paste_artifact",
        )

        cases.forEach { (dispatcher, tool) ->
            val execution = dispatcher.dispatch(JOB, tool, JsonObject(emptyMap()))
            assertEquals("provider_contract_failure", execution.response.stringValue("code"))
            assertEquals(
                "invalid_dependency_provider_receipt",
                execution.response.stringValue("provider_route_error"),
            )
        }
    }

    @Test
    fun providerDataCannotOverwriteTypedEnvelopeFields() {
        val response = toolResponse(
            job = JOB,
            requestedTool = "system_query",
            capability = "system_query",
            provider = "android_app_api",
            providerState = CapabilityState.READY,
            code = "ok",
            observationGeneration = 0,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            data = buildJsonObject {
                put("code", "untrusted-provider-code")
                put("requested_tool", "rerouted-tool")
                put("provider", "spoofed-provider")
            },
        )

        assertEquals("ok", response.stringValue("code"))
        assertEquals("system_query", response.stringValue("requested_tool"))
        assertEquals("android_app_api", response.stringValue("provider"))
    }

    private fun generatedCatalogNames(): List<String> {
        val catalog = findGeneratedCatalog()
        val root = Json.parseToJsonElement(catalog.readText()).jsonObject
        return root.getValue("functionDeclarations").jsonArray.map { declaration ->
            declaration.jsonObject.getValue("name").jsonPrimitive.content
        }
    }

    private fun findGeneratedCatalog(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        var directory: File? = File(workingDirectory).absoluteFile
        while (directory != null) {
            val candidate = File(directory, GENERATED_CATALOG_PATH)
            if (candidate.isFile) return candidate
            directory = directory.parentFile
        }
        error("Missing generated Phone Control catalog at $GENERATED_CATALOG_PATH")
    }

    private fun JsonObject.stringValue(name: String): String =
        getValue(name).jsonPrimitive.content

    private fun dispatcherReturning(
        tool: String,
        provider: String,
        capability: String = PhoneControlToolRegistry.byName.getValue(tool).capability,
        effect: EffectCertainty,
        providerState: CapabilityState = CapabilityState.READY,
        code: String = "ok",
        mutating: Boolean? = null,
        data: JsonObject = JsonObject(emptyMap()),
    ) = PhoneControlToolDispatcher(
        executor = PhoneControlHandlerExecutor { handler, job, requestedTool, _ ->
            assertEquals(tool, requestedTool)
            PhoneControlToolExecution(
                response = toolResponse(
                    job = job,
                    requestedTool = requestedTool,
                    capability = capability,
                    provider = provider,
                    providerState = providerState,
                    code = code,
                    observationGeneration = 0,
                    effect = effect,
                    snapshotInvalidated = effect.effectMayHaveOccurred == true,
                    data = data,
                ),
                mutating = mutating
                    ?: (handler.mutating && effect.effectMayHaveOccurred == true),
            )
        },
        providerRouter = ROUTER,
        failureReporter = PhoneControlToolFailureReporter { _, _, _ ->
            error("no provider failure expected")
        },
    )

    private companion object {
        const val DISPATCHED = "test_dispatched"
        const val GENERATED_CATALOG_PATH =
            "androidApp/build/generated/phoneControlContract/assets/phone_control/catalog.json"
        val ROUTER = PhoneControlAuthorityFixture.load().let {
            ProviderRouter(it.providers, it.routes)
        }
        val JOB = PhoneControlToolJobContext(
            turnId = 7,
            jobId = "job-dispatch-test",
            responseGeneration = 11,
        )
    }
}

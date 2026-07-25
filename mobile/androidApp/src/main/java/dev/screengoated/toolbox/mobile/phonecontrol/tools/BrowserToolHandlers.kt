package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.AndroidBrowserProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.AndroidChromeCdpProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.BrowserTargetBaselineResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.BrowserProviderOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.PublicWebResearchProvider
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class BrowserToolHandlers(
    private val provider: AndroidBrowserProvider,
    private val deep: AndroidChromeCdpProvider,
    private val research: PublicWebResearchProvider,
) {
    suspend fun setup(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        outcome(job, "browser_setup", "browser_authenticated_navigation", provider.status(setup = true))

    suspend fun status(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        deep.status().let { cdp ->
            if (cdp.code == "ok") {
                outcome(job, "browser_status", "browser_semantic", cdp)
            } else {
                val baseline = provider.status(setup = false)
                outcome(
                    job,
                    "browser_status",
                    "browser_semantic",
                    baseline.copy(data = buildJsonObject {
                        baseline.data.forEach { (key, value) -> put(key, value) }
                        put("browser_cdp", buildJsonObject {
                            put("state", cdp.state.wireName)
                            put("code", cdp.code)
                            put("retryable", cdp.retryable)
                            cdp.requiredUserStep?.let { put("required_user_step", it) }
                        })
                    }),
                )
            }
        }

    suspend fun readPage(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        outcome(
            job,
            "browser_read_page",
            "browser_semantic",
            if (deep.hasBoundTarget()) deep.readPage(includePreview = true) else {
                provider.capture(includePreview = true)
            },
        )

    suspend fun extractPage(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        outcome(
            job,
            "browser_extract_page",
            "browser_semantic",
            if (deep.hasBoundTarget()) deep.readPage(includePreview = false) else {
                provider.capture(includePreview = false)
            },
        )

    suspend fun reset(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        outcome(job, "browser_reset", "browser_devtools", deep.reset())

    suspend fun research(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val result = research.research(args)
        return PhoneControlToolExecution(
            response = toolResponse(
                job = job,
                requestedTool = "research_web",
                capability = "public_web_research",
                provider = "direct_web_research",
                providerState = result.state,
                code = result.code,
                observationGeneration = 0,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                retryable = result.retryable,
                data = result.data,
            ),
            mutating = false,
        )
    }

    suspend fun waitFor(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val selector = args.string("selector")
            ?: return invalidArgs(job, "browser_wait_for", "browser_wait_for requires selector")
        val timeoutMs = args.int("timeout_ms")?.toLong() ?: DEFAULT_WAIT_TIMEOUT_MS
        return outcome(
            job,
            "browser_wait_for",
            "browser_semantic",
            deep.waitFor(selector, timeoutMs),
        )
    }

    suspend fun eval(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val code = args.string("code")
            ?: return invalidArgs(job, "browser_eval", "browser_eval requires code")
        return outcome(job, "browser_eval", "browser_devtools", deep.eval(code))
    }

    suspend fun navigate(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val url = args.string("url")
            ?: return invalidArgs(job, "browser_navigate", "browser_navigate requires url")
        val lifetime = args.string("lifetime")
            ?: return invalidArgs(job, "browser_navigate", "browser_navigate requires lifetime")
        return outcome(
            job,
            "browser_navigate",
            "browser_authenticated_navigation",
            when {
                lifetime == "turn" -> deep.openTab(url, lifetime, job.turnId)
                deep.hasBoundTarget() -> deep.navigate(url)
                else -> navigateCustomTabWithDiscovery(url, lifetime)
            },
        )
    }

    private suspend fun navigateCustomTabWithDiscovery(
        url: String,
        lifetime: String,
    ): BrowserProviderOutcome {
        val baseline = deep.targetBaseline()
        var attachment: BrowserProviderOutcome? = null
        val navigation = provider.navigate(url, lifetime) {
            val exactBaseline = (baseline as? BrowserTargetBaselineResult.Ready)?.baseline
            if (exactBaseline != null) {
                attachment = deep.attachLaunchedTarget(url, exactBaseline)
            }
        }
        val attached = attachment ?: return navigation
        return navigation.copy(data = buildJsonObject {
            navigation.data.forEach { (key, value) -> put(key, value) }
            put("browser_cdp_attachment", buildJsonObject {
                put("state", attached.state.wireName)
                put("code", attached.code)
                put("exact_target_bound", attached.code == "ok")
                attached.data["target"]?.let { put("target", it) }
            })
        })
    }

    suspend fun history(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val direction = args.string("direction")
            ?: return invalidArgs(job, "browser_history", "browser_history requires direction")
        return outcome(
            job,
            "browser_history",
            "browser_authenticated_navigation",
            if (deep.hasBoundTarget()) deep.history(direction) else provider.history(direction),
        )
    }

    suspend fun openTab(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val url = args.string("url")
            ?: return invalidArgs(job, "browser_open_tab", "browser_open_tab requires url")
        val lifetime = args.string("lifetime") ?: "persistent"
        return outcome(
            job,
            "browser_open_tab",
            "browser_devtools",
            deep.openTab(url, lifetime, job.turnId),
        )
    }

    suspend fun upload(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val selector = args.string("selector")
            ?: return invalidArgs(job, "browser_upload", "browser_upload requires selector")
        val path = args.string("path")
            ?: return invalidArgs(job, "browser_upload", "browser_upload requires path")
        return outcome(job, "browser_upload", "browser_devtools", deep.upload(selector, path))
    }

    suspend fun tabs(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        outcome(job, "browser_tabs", "browser_devtools", deep.tabs())

    suspend fun switchTab(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val tabId = args.int("tab_id")
            ?: return invalidArgs(job, "browser_switch_tab", "browser_switch_tab requires tab_id")
        return outcome(job, "browser_switch_tab", "browser_devtools", deep.switchTab(tabId))
    }

    suspend fun closeTab(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val tabId = args.int("tab_id")
            ?: return invalidArgs(job, "browser_close_tab", "browser_close_tab requires tab_id")
        return outcome(job, "browser_close_tab", "browser_devtools", deep.closeTab(tabId))
    }

    suspend fun network(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = outcome(
        job,
        "browser_network",
        "browser_devtools",
        deep.network(args.string("filter")),
    )

    suspend fun console(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        outcome(job, "browser_console", "browser_devtools", deep.console())

    private fun outcome(
        job: PhoneControlToolJobContext,
        requestedTool: String,
        capability: String,
        result: BrowserProviderOutcome,
    ): PhoneControlToolExecution = browserProviderExecution(job, requestedTool, capability, result)

    private companion object {
        const val DEFAULT_WAIT_TIMEOUT_MS = 10_000L
    }
}

internal fun browserProviderExecution(
    job: PhoneControlToolJobContext,
    requestedTool: String,
    capability: String,
    result: BrowserProviderOutcome,
): PhoneControlToolExecution {
    val receiptData = buildJsonObject {
        result.data.forEach { (key, value) -> put(key, value) }
        put("provider_role", result.providerRole.wireName)
    }
    return PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = requestedTool,
            capability = capability,
            provider = result.providerId,
            providerState = result.state,
            code = result.code,
            observationGeneration = result.observationGeneration,
            effect = result.effect,
            snapshotInvalidated = result.snapshotInvalidated,
            retryable = result.retryable,
            requiredUserStep = result.requiredUserStep,
            freshObservationRequired = result.freshObservationRequired.takeIf { it },
            data = receiptData,
        ),
        mutating = result.effect.effectMayHaveOccurred == true,
        refreshScreenFrame = result.snapshotInvalidated,
    )
}

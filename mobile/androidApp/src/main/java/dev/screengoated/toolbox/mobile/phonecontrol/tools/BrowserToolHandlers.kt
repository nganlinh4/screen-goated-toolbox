package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.AndroidBrowserProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.BrowserProviderOutcome
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class BrowserToolHandlers(
    private val provider: AndroidBrowserProvider,
) {
    suspend fun setup(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        outcome(job, "browser_setup", "browser_authenticated_navigation", provider.status(setup = true))

    suspend fun status(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        outcome(job, "browser_status", "browser_semantic", provider.status(setup = false))

    suspend fun readPage(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        outcome(job, "browser_read_page", "browser_semantic", provider.capture(includePreview = true))

    suspend fun extractPage(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        outcome(job, "browser_extract_page", "browser_semantic", provider.capture(includePreview = false))

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
            provider.navigate(url, lifetime),
        )
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
            provider.history(direction),
        )
    }

    private fun outcome(
        job: PhoneControlToolJobContext,
        requestedTool: String,
        capability: String,
        result: BrowserProviderOutcome,
    ): PhoneControlToolExecution = browserProviderExecution(job, requestedTool, capability, result)
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

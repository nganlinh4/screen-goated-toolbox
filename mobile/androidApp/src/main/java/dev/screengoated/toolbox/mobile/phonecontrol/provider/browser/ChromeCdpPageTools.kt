package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

internal suspend fun ChromeCdpController.readPage(
    includePreview: Boolean,
): BrowserProviderOutcome = mutex.withLock {
    val current = bound ?: return@withLock noBoundTarget()
    val loaderBefore = when (val loader = mainLoaderId(current)) {
        is MainLoaderResult.Failure -> return@withLock loader.outcome
        is MainLoaderResult.Ready -> loader.loaderId
    }
    val capture = when (val result = evaluate(current, PAGE_CAPTURE_EXPRESSION, false)) {
        is CdpEvalResult.Failure -> return@withLock result.outcome
        is CdpEvalResult.Value -> result.value as? JsonObject ?: return@withLock failure(
            "browser_capture_invalid",
            CapabilityState.DEGRADED,
            "Chrome returned no structured page capture.",
        )
    }
    val text = capture["text"]?.jsonPrimitive?.contentOrNull.orEmpty().take(MAX_PAGE_TEXT_CHARS)
    val artifact = artifacts.put(
        text.toByteArray(Charsets.UTF_8),
        "text/plain; charset=utf-8",
        "browser-cdp-page.txt",
    )
    val loaderId = when (val loader = mainLoaderId(current)) {
        is MainLoaderResult.Failure -> return@withLock loader.outcome
        is MainLoaderResult.Ready -> loader.loaderId
    }
    if (loaderId != loaderBefore) {
        return@withLock failure(
            "browser_capture_raced_navigation",
            CapabilityState.DEGRADED,
            "The exact tab navigated while its page was being captured.",
            retryable = true,
            freshObservationRequired = true,
        )
    }
    val refreshed = refreshBinding(current, capture, loaderId)
    success(data = buildJsonObject {
        put("page", buildJsonObject {
            put("title", capture["title"]?.jsonPrimitive?.contentOrNull.orEmpty())
            put("url", capture["url"]?.jsonPrimitive?.contentOrNull.orEmpty())
            put("char_count", text.length)
            put("capture_complete", text.length < MAX_PAGE_TEXT_CHARS)
            put("ready_state", capture["readyState"]?.jsonPrimitive?.contentOrNull.orEmpty())
            if (includePreview) put("text", text.take(READ_PREVIEW_CHARS))
            if (includePreview) put("truncated", text.length > READ_PREVIEW_CHARS)
        })
        put("artifact", artifact.info())
        put("target", refreshed.toWireJson())
        put("credential_context_kind", "attached_browser_tab")
        put("cookie_access", false)
    })
}

internal suspend fun ChromeCdpController.navigate(url: String): BrowserProviderOutcome =
    mutex.withLock {
        val uri = browserHttpUri(url) ?: return@withLock failure(
            "invalid_url",
            CapabilityState.READY,
            "browser_navigate requires an absolute public http or https URL.",
            retryable = false,
        )
        val current = bound ?: return@withLock noBoundTarget()
        val command = current.session.send(
            "Page.navigate",
            buildJsonObject { put("url", uri.toString()) },
        )
        if (command is CdpCommandResult.Failure) return@withLock command.toOutcome()
        val result = (command as CdpCommandResult.Success).result
        if (result["errorText"]?.jsonPrimitive?.contentOrNull?.isNotBlank() == true) {
            return@withLock failure(
                "navigation_rejected",
                CapabilityState.DEGRADED,
                "Chrome rejected navigation.",
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
                snapshotInvalidated = true,
            )
        }
        val expectedLoader = result["loaderId"]?.jsonPrimitive?.contentOrNull
        val observed = awaitNavigation(
            current = current,
            requestedUrl = uri.toString(),
            expectedLoader = expectedLoader,
            previousLoader = current.binding.loaderId,
        )
        val verified = observed != null
        val binding = observed?.let {
            refreshBinding(current, it.capture, it.loaderId)
        } ?: current.binding
        success(
            code = if (verified) "ok" else "navigation_postcondition_unverified",
            state = if (verified) CapabilityState.READY else CapabilityState.DEGRADED,
            effect = if (verified) EffectCertainty.VERIFIED else EffectCertainty.MAY_HAVE_OCCURRED,
            snapshotInvalidated = true,
            retryable = !verified,
            data = buildJsonObject {
                put("requested_url", uri.toString())
                observed?.capture?.get("url")?.let { put("observed_url", it) }
                put("target", binding.toWireJson())
            },
        )
    }

internal suspend fun ChromeCdpController.history(direction: String): BrowserProviderOutcome =
    mutex.withLock {
        if (direction !in setOf("back", "forward")) {
            return@withLock failure(
                "invalid_direction",
                CapabilityState.READY,
                "direction must be back or forward.",
                retryable = false,
            )
        }
        val current = bound ?: return@withLock noBoundTarget()
        val history = current.session.send("Page.getNavigationHistory")
        if (history is CdpCommandResult.Failure) return@withLock history.toOutcome()
        val data = (history as CdpCommandResult.Success).result
        val index = data["currentIndex"]?.jsonPrimitive?.intOrNull ?: return@withLock failure(
            "history_state_unavailable",
            CapabilityState.DEGRADED,
            "Chrome returned no current history index.",
        )
        val entries = data["entries"] as? JsonArray ?: JsonArray(emptyList())
        val desired = if (direction == "back") index - 1 else index + 1
        val entryId = (entries.getOrNull(desired) as? JsonObject)
            ?.get("id")
            ?.jsonPrimitive
            ?.intOrNull
            ?: return@withLock failure(
                "history_boundary",
                CapabilityState.READY,
                "There is no history entry in that direction.",
                retryable = false,
            )
        val requestedUrl = (entries.getOrNull(desired) as? JsonObject)
            ?.get("url")
            ?.jsonPrimitive
            ?.contentOrNull
            .orEmpty()
        val moved = current.session.send(
            "Page.navigateToHistoryEntry",
            buildJsonObject { put("entryId", entryId) },
        )
        if (moved is CdpCommandResult.Failure) return@withLock moved.toOutcome()
        val observed = awaitNavigation(
            current = current,
            requestedUrl = requestedUrl,
            expectedLoader = null,
            previousLoader = current.binding.loaderId,
        )
        val verified = observed != null
        val binding = observed?.let {
            refreshBinding(current, it.capture, it.loaderId)
        } ?: current.binding
        success(
            code = if (verified) "ok" else "history_postcondition_unverified",
            state = if (verified) CapabilityState.READY else CapabilityState.DEGRADED,
            effect = if (verified) EffectCertainty.VERIFIED else EffectCertainty.MAY_HAVE_OCCURRED,
            snapshotInvalidated = true,
            retryable = !verified,
            data = buildJsonObject {
                put("direction", direction)
                observed?.capture?.get("url")?.let { put("observed_url", it) }
                put("target", binding.toWireJson())
            },
        )
    }

internal suspend fun ChromeCdpController.waitFor(
    selector: String,
    timeoutMs: Long,
): BrowserProviderOutcome = mutex.withLock {
    if (selector.isBlank() || selector.length > MAX_SELECTOR_CHARS) {
        return@withLock failure(
            "invalid_selector",
            CapabilityState.READY,
            "selector is blank or too long.",
            retryable = false,
        )
    }
    val current = bound ?: return@withLock noBoundTarget()
    ensureCurrentDocument(current)?.let { return@withLock it }
    val deadline = android.os.SystemClock.elapsedRealtime() +
        timeoutMs.coerceIn(MIN_WAIT_MS, MAX_WAIT_MS)
    val expression = "document.querySelector(${JsonPrimitive(selector)}) !== null"
    do {
        when (val value = evaluate(current, expression, false)) {
            is CdpEvalResult.Value -> if (value.value.jsonPrimitive.booleanOrNull == true) {
                return@withLock success(data = buildJsonObject {
                    put("found", true)
                    put("selector", selector)
                    put("target", current.binding.toWireJson())
                })
            }
            is CdpEvalResult.Failure -> return@withLock value.outcome
        }
        delay(WAIT_POLL_MS)
    } while (android.os.SystemClock.elapsedRealtime() < deadline)
    failure(
        "browser_wait_timeout",
        CapabilityState.DEGRADED,
        "The selector did not appear before the timeout.",
        retryable = true,
    )
}

internal suspend fun ChromeCdpController.eval(code: String): BrowserProviderOutcome =
    mutex.withLock {
        if (code.isBlank() || code.length > MAX_EVAL_CHARS) {
            return@withLock failure(
                "invalid_code",
                CapabilityState.READY,
                "code is blank or too long.",
                retryable = false,
            )
        }
        val current = bound ?: return@withLock noBoundTarget()
        ensureCurrentDocument(current)?.let { return@withLock it }
        when (val result = evaluate(current, code, true)) {
            is CdpEvalResult.Failure -> result.outcome
            is CdpEvalResult.Value -> success(
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
                snapshotInvalidated = true,
                data = buildJsonObject {
                    put("result", result.value)
                    put("target", current.binding.toWireJson())
                },
            )
        }
    }

internal suspend fun ChromeCdpController.upload(
    selector: String,
    path: String,
): BrowserProviderOutcome = mutex.withLock {
    if (selector.isBlank() || selector.length > MAX_SELECTOR_CHARS || !path.startsWith('/')) {
        return@withLock failure(
            "invalid_upload_request",
            CapabilityState.READY,
            "browser_upload requires a selector and an absolute path.",
            retryable = false,
        )
    }
    val current = bound ?: return@withLock noBoundTarget()
    ensureCurrentDocument(current)?.let { return@withLock it }
    val document = current.session.send("DOM.getDocument")
    if (document is CdpCommandResult.Failure) return@withLock document.toOutcome()
    val rootId = ((document as CdpCommandResult.Success).result["root"] as? JsonObject)
        ?.get("nodeId")
        ?.jsonPrimitive
        ?.intOrNull
        ?: return@withLock failure(
            "browser_dom_unavailable",
            CapabilityState.DEGRADED,
            "Chrome returned no document root.",
        )
    val queried = current.session.send(
        "DOM.querySelector",
        buildJsonObject {
            put("nodeId", rootId)
            put("selector", selector)
        },
    )
    if (queried is CdpCommandResult.Failure) return@withLock queried.toOutcome()
    val nodeId = (queried as CdpCommandResult.Success).result["nodeId"]
        ?.jsonPrimitive
        ?.intOrNull
        ?.takeIf { it > 0 }
        ?: return@withLock failure(
            "browser_upload_target_not_found",
            CapabilityState.READY,
            "No matching file input exists in the exact target.",
            retryable = true,
        )
    val uploaded = current.session.send(
        "DOM.setFileInputFiles",
        buildJsonObject {
            put("nodeId", nodeId)
            put("files", buildJsonArray { add(JsonPrimitive(path)) })
        },
    )
    if (uploaded is CdpCommandResult.Failure) return@withLock uploaded.toOutcome()
    success(
        effect = EffectCertainty.MAY_HAVE_OCCURRED,
        snapshotInvalidated = true,
        data = buildJsonObject {
            put("selector", selector)
            put("file_count", 1)
            put("path_returned", false)
            put("target", current.binding.toWireJson())
        },
    )
}

internal suspend fun ChromeCdpController.network(filter: String?): BrowserProviderOutcome =
    mutex.withLock {
        val current = bound ?: return@withLock noBoundTarget()
        ensureCurrentDocument(current)?.let { return@withLock it }
        val enabled = current.session.send("Network.enable")
        if (enabled is CdpCommandResult.Failure) return@withLock enabled.toOutcome()
        success(data = buildJsonObject {
            put("events", summarizeNetworkEvents(current.session.networkEvents(filter)))
            put("target", current.binding.toWireJson())
        })
    }

internal suspend fun ChromeCdpController.console(): BrowserProviderOutcome = mutex.withLock {
    val current = bound ?: return@withLock noBoundTarget()
    ensureCurrentDocument(current)?.let { return@withLock it }
    for (method in listOf("Runtime.enable", "Log.enable")) {
        val enabled = current.session.send(method)
        if (enabled is CdpCommandResult.Failure) return@withLock enabled.toOutcome()
    }
    success(data = buildJsonObject {
        put("events", summarizeConsoleEvents(current.session.consoleEvents()))
        put("target", current.binding.toWireJson())
    })
}

private suspend fun ChromeCdpController.evaluate(
    current: ChromeCdpController.BoundTarget,
    expression: String,
    userGesture: Boolean,
): CdpEvalResult {
    val command = current.session.send(
        "Runtime.evaluate",
        buildJsonObject {
            put("expression", expression)
            put("returnByValue", true)
            put("awaitPromise", true)
            put("userGesture", userGesture)
        },
    )
    if (command is CdpCommandResult.Failure) return CdpEvalResult.Failure(command.toOutcome())
    val result = (command as CdpCommandResult.Success).result
    if (result["exceptionDetails"] != null) {
        return CdpEvalResult.Failure(
            failure(
                "browser_eval_exception",
                CapabilityState.READY,
                "The page expression threw an exception.",
                effect = if (userGesture) {
                    EffectCertainty.MAY_HAVE_OCCURRED
                } else {
                    EffectCertainty.PROVEN_NO_EFFECT
                },
                snapshotInvalidated = userGesture,
                retryable = false,
            ),
        )
    }
    val remote = result["result"] as? JsonObject
    return CdpEvalResult.Value(remote?.get("value") ?: JsonNull)
}

private suspend fun ChromeCdpController.ensureCurrentDocument(
    current: ChromeCdpController.BoundTarget,
): BrowserProviderOutcome? = when (val loader = mainLoaderId(current)) {
    is MainLoaderResult.Failure -> loader.outcome
    is MainLoaderResult.Ready -> if (loader.loaderId == current.binding.loaderId) {
        null
    } else {
        failure(
            "stale_document",
            CapabilityState.DEGRADED,
            "The exact tab navigated after its document binding. Read the page again.",
            retryable = true,
            freshObservationRequired = true,
        )
    }
}

private suspend fun ChromeCdpController.mainLoaderId(
    current: ChromeCdpController.BoundTarget,
): MainLoaderResult {
    val frameTree = current.session.send("Page.getFrameTree")
    if (frameTree is CdpCommandResult.Failure) {
        return MainLoaderResult.Failure(frameTree.toOutcome())
    }
    val loaderId = (((frameTree as CdpCommandResult.Success).result["frameTree"] as? JsonObject)
        ?.get("frame") as? JsonObject)
        ?.get("loaderId")
        ?.jsonPrimitive
        ?.contentOrNull
        ?: return MainLoaderResult.Failure(
            failure(
                "browser_document_generation_unavailable",
                CapabilityState.DEGRADED,
                "Chrome returned no main-document generation.",
                retryable = true,
                freshObservationRequired = true,
            ),
        )
    return MainLoaderResult.Ready(loaderId)
}

private suspend fun ChromeCdpController.awaitNavigation(
    current: ChromeCdpController.BoundTarget,
    requestedUrl: String,
    expectedLoader: String?,
    previousLoader: String?,
): NavigationObservation? {
    repeat(PAGE_VERIFY_ATTEMPTS) {
        delay(PAGE_VERIFY_DELAY_MS)
        val result = evaluate(current, PAGE_STATE_EXPRESSION, false)
        val value = (result as? CdpEvalResult.Value)?.value as? JsonObject
        val frameTree = current.session.send("Page.getFrameTree")
        val frame = ((frameTree as? CdpCommandResult.Success)?.result?.get("frameTree") as? JsonObject)
            ?.get("frame") as? JsonObject
        val loaderId = frame?.get("loaderId")?.jsonPrimitive?.contentOrNull
        val observedUrl = value?.get("url")?.jsonPrimitive?.contentOrNull
        val ready = value?.get("readyState")?.jsonPrimitive?.contentOrNull in
            setOf("interactive", "complete")
        val generationMatches = when {
            expectedLoader != null -> loaderId == expectedLoader
            previousLoader != null && loaderId != null -> loaderId != previousLoader
            else -> observedUrl == requestedUrl
        }
        if (
            value != null &&
            ready &&
            browserHttpUri(observedUrl.orEmpty()) != null &&
            generationMatches
        ) {
            return NavigationObservation(value, loaderId)
        }
    }
    return null
}

private fun ChromeCdpController.refreshBinding(
    current: ChromeCdpController.BoundTarget,
    capture: JsonObject,
    loaderId: String?,
): ChromeDeepBinding {
    val refreshedTarget = current.binding.target.copy(
        title = capture["title"]?.jsonPrimitive?.contentOrNull
            ?.take(512) ?: current.binding.target.title,
        url = capture["url"]?.jsonPrimitive?.contentOrNull
            ?.take(4_096) ?: current.binding.target.url,
    )
    val generation = observationGeneration.incrementAndGet()
    val value = current.binding.copy(
        target = refreshedTarget,
        documentId = loaderId ?: current.binding.documentId,
        loaderId = loaderId ?: current.binding.loaderId,
        observationGeneration = generation,
    )
    bound = current.copy(binding = value)
    return value
}

private sealed interface CdpEvalResult {
    data class Value(val value: JsonElement) : CdpEvalResult
    data class Failure(val outcome: BrowserProviderOutcome) : CdpEvalResult
}

private sealed interface MainLoaderResult {
    data class Ready(val loaderId: String) : MainLoaderResult
    data class Failure(val outcome: BrowserProviderOutcome) : MainLoaderResult
}

private data class NavigationObservation(
    val capture: JsonObject,
    val loaderId: String?,
)

private fun summarizeNetworkEvents(events: List<JsonObject>): JsonArray = buildJsonArray {
    events.forEach { event ->
        val method = event["method"]?.jsonPrimitive?.contentOrNull.orEmpty()
        val params = event["params"] as? JsonObject ?: JsonObject(emptyMap())
        val response = params["response"] as? JsonObject
        val request = params["request"] as? JsonObject
        add(buildJsonObject {
            put("method", method)
            params["requestId"]?.let { put("request_id", it) }
            (response?.get("url") ?: request?.get("url"))?.let { put("url", it) }
            response?.get("status")?.let { put("status", it) }
            response?.get("mimeType")?.let { put("mime_type", it) }
            params["type"]?.let { put("resource_type", it) }
            params["errorText"]?.let { put("error", it) }
        })
    }
}

private fun summarizeConsoleEvents(events: List<JsonObject>): JsonArray = buildJsonArray {
    events.forEach { event ->
        val method = event["method"]?.jsonPrimitive?.contentOrNull.orEmpty()
        val params = event["params"] as? JsonObject ?: JsonObject(emptyMap())
        add(buildJsonObject {
            put("method", method)
            params["level"]?.let { put("level", it) }
            params["text"]?.let { put("text", it) }
            params["type"]?.let { put("type", it) }
            params["timestamp"]?.let { put("timestamp", it) }
        })
    }
}

private const val MAX_PAGE_TEXT_CHARS = 128_000
private const val READ_PREVIEW_CHARS = 24_000
private const val MAX_SELECTOR_CHARS = 4_096
private const val MAX_EVAL_CHARS = 64_000
private const val MIN_WAIT_MS = 100L
private const val MAX_WAIT_MS = 30_000L
private const val WAIT_POLL_MS = 150L
private const val PAGE_VERIFY_ATTEMPTS = 30
private const val PAGE_VERIFY_DELAY_MS = 150L
private const val PAGE_STATE_EXPRESSION =
    "({url:location.href,readyState:document.readyState})"
private const val PAGE_CAPTURE_EXPRESSION =
    "(()=>{const text=(document.body?.innerText||'').slice(0,128000);" +
        "return {title:document.title,url:location.href,text," +
        "readyState:document.readyState,documentId:" +
        "(document.documentElement?.baseURI||location.href)}})()"

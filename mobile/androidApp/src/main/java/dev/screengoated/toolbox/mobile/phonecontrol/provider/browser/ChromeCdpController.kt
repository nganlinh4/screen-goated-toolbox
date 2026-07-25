package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.PhoneControlArtifactStore
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import java.io.Closeable
import java.net.URI
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

internal class ChromeCdpController(
    context: Context,
    internal val artifacts: PhoneControlArtifactStore,
) : Closeable {
    internal val transport = AndroidChromeDevToolsTransport(context)
    internal val mutex = Mutex()
    private val handlesByTarget = LinkedHashMap<String, Int>()
    private val targetsByHandle = LinkedHashMap<Int, String>()
    private val ownedTargets = LinkedHashMap<String, OwnedTarget>()
    private val nextHandle = AtomicInteger(1)
    private val closed = AtomicBoolean(false)
    internal val observationGeneration = AtomicLong(0)
    internal var bound: BoundTarget? = null

    fun hasBoundTarget(): Boolean = bound != null

    suspend fun status(): BrowserProviderOutcome = mutex.withLock {
        when (val targets = freshTargets()) {
            is BrowserTransportResult.Failure -> targets.toOutcome()
            is BrowserTransportResult.Ready -> success(
                data = buildJsonObject {
                    put("target_count", targets.value.size)
                    put("exact_target_bound", bound != null)
                    bound?.let { put("bound_target", it.binding.toWireJson()) }
                    put("credential_context_kind", "attached_browser_tab")
                    put("cookie_access", false)
                    put("browser_chrome_control", false)
                    transport.authorityProviderId?.let { put("transport_authority", it) }
                },
            )
        }
    }

    suspend fun reset(): BrowserProviderOutcome = mutex.withLock {
        retireBound()
        handlesByTarget.clear()
        targetsByHandle.clear()
        success(data = buildJsonObject {
            put("session_reset", true)
            put("browser_targets_closed", 0)
        })
    }

    suspend fun tabs(): BrowserProviderOutcome = mutex.withLock {
        when (val result = freshTargets()) {
            is BrowserTransportResult.Failure -> result.toOutcome()
            is BrowserTransportResult.Ready -> {
                val selected = bound?.binding?.target?.targetId
                success(data = buildJsonObject {
                    put("tabs", buildJsonArray {
                        result.value.forEach { target ->
                            add(buildJsonObject {
                                put("id", handleFor(target.targetId))
                                put("title", target.title)
                                put("url", target.url)
                                put("active", target.targetId == selected)
                                put("credential_context_kind", "attached_browser_tab")
                            })
                        }
                    })
                })
            }
        }
    }

    suspend fun targetBaseline(): BrowserTargetBaselineResult = mutex.withLock {
        when (val targets = freshTargets()) {
            is BrowserTransportResult.Failure ->
                BrowserTargetBaselineResult.Failure(targets.toOutcome())
            is BrowserTransportResult.Ready -> BrowserTargetBaselineResult.Ready(
                ChromeTargetBaseline(
                    targets.value.associate { it.targetId to it.url },
                ),
            )
        }
    }

    suspend fun attachLaunchedTarget(
        url: String,
        baseline: ChromeTargetBaseline,
    ): BrowserProviderOutcome = mutex.withLock {
        val requested = browserHttpUri(url) ?: return@withLock failure(
            "invalid_url",
            CapabilityState.READY,
            "Custom Tab target discovery requires an absolute http or https URL.",
            retryable = false,
        )
        repeat(TARGET_DISCOVERY_ATTEMPTS) { attempt ->
            if (attempt > 0) delay(TARGET_DISCOVERY_DELAY_MS)
            when (val targets = freshTargets()) {
                is BrowserTransportResult.Failure -> return@withLock targets.toOutcome()
                is BrowserTransportResult.Ready -> when (
                    val resolved = resolveLaunchedChromeTarget(requested, baseline, targets.value)
                ) {
                    LaunchedChromeTargetResolution.Ambiguous -> return@withLock failure(
                        "browser_target_ambiguous",
                        CapabilityState.DEGRADED,
                        "More than one changed Chrome target matches the launched URL.",
                        retryable = true,
                        freshObservationRequired = true,
                    )
                    is LaunchedChromeTargetResolution.Exact -> {
                        return@withLock when (val attached = bind(resolved.target)) {
                            is BindResult.Failure -> attached.outcome
                            is BindResult.Ready -> success(data = buildJsonObject {
                                put("attached", true)
                                put("discovery", "exact_launch_delta_and_url")
                                put("target", attached.binding.toWireJson())
                            })
                        }
                    }
                    LaunchedChromeTargetResolution.Pending -> Unit
                }
            }
        }
        failure(
            "browser_target_not_discovered",
            CapabilityState.UNAVAILABLE,
            "The launched browser surface did not expose one exact matching Chrome target.",
            retryable = true,
            freshObservationRequired = true,
        )
    }

    suspend fun openTab(
        url: String,
        lifetime: String,
        turnId: Long,
    ): BrowserProviderOutcome = mutex.withLock {
        if (ownedTargets.size >= MAX_OWNED_TARGETS) {
            return@withLock failure(
                "browser_owned_target_limit",
                CapabilityState.DEGRADED,
                "The bounded Phone Control browser-target limit is already in use.",
                retryable = true,
            )
        }
        val uri = browserHttpUri(url) ?: return@withLock failure(
            "invalid_url",
            CapabilityState.READY,
            "browser_open_tab requires an absolute http or https URL without embedded credentials.",
            retryable = false,
        )
        if (lifetime !in setOf("turn", "persistent")) {
            return@withLock failure(
                "unsupported_tab_lifetime",
                CapabilityState.UNSUPPORTED,
                "lifetime must be turn or persistent.",
                retryable = false,
            )
        }
        val opened = transport.requestJson(
            method = "PUT",
            path = "/json/new",
            query = mapOf("url" to uri.toString()),
        )
        val target = (opened as? BrowserTransportResult.Ready)
            ?.value
            ?.let(::parseSingleTarget)
            ?: return@withLock when (opened) {
                is BrowserTransportResult.Failure -> opened.toOutcome()
                is BrowserTransportResult.Ready -> failure(
                    "browser_target_invalid",
                    CapabilityState.DEGRADED,
                    "Chrome did not return an exact page target.",
                    effect = EffectCertainty.MAY_HAVE_OCCURRED,
                    snapshotInvalidated = true,
                    freshObservationRequired = true,
                )
            }
        val handle = handleFor(target.targetId)
        ownedTargets[target.targetId] = OwnedTarget(turnId, lifetime)
        val binding = when (val result = bind(target)) {
            is BindResult.Ready -> result.binding
            is BindResult.Failure -> return@withLock result.outcome.copy(
                code = "browser_target_bind_failed",
                data = buildJsonObject {
                    put("cause_code", result.outcome.code)
                    put("tab_id", handle)
                    put("target_id_present", true)
                    put("lifetime", lifetime)
                },
                effect = EffectCertainty.VERIFIED,
                snapshotInvalidated = true,
                freshObservationRequired = true,
            )
        }
        success(
            effect = EffectCertainty.VERIFIED,
            snapshotInvalidated = true,
            data = buildJsonObject {
                put("tab_id", binding.handle)
                put("url", target.url)
                put("lifetime", lifetime)
                put("target", binding.toWireJson())
            },
        )
    }

    suspend fun switchTab(handle: Int): BrowserProviderOutcome = mutex.withLock {
        val target = when (val exact = exactTarget(handle)) {
            is ExactTargetResult.Ready -> exact.target
            is ExactTargetResult.Failure -> return@withLock exact.outcome
        }
        when (val activated = transport.requestStatus("GET", "/json/activate/${target.targetId}")) {
            is BrowserTransportResult.Failure -> return@withLock activated.toOutcome()
            is BrowserTransportResult.Ready -> Unit
        }
        when (val result = bind(target)) {
            is BindResult.Failure -> result.outcome.copy(
                code = "browser_target_bind_failed_after_activate",
                data = buildJsonObject {
                    put("cause_code", result.outcome.code)
                    put("tab_id", handle)
                    put("target_id_present", true)
                },
                effect = EffectCertainty.VERIFIED,
                snapshotInvalidated = true,
                freshObservationRequired = true,
            )
            is BindResult.Ready -> success(
                effect = EffectCertainty.VERIFIED,
                snapshotInvalidated = true,
                data = buildJsonObject {
                    put("tab_id", handle)
                    put("target", result.binding.toWireJson())
                },
            )
        }
    }

    suspend fun closeTab(handle: Int): BrowserProviderOutcome = mutex.withLock {
        val target = when (val exact = exactTarget(handle)) {
            is ExactTargetResult.Ready -> exact.target
            is ExactTargetResult.Failure -> return@withLock exact.outcome
        }
        val dispatched = transport.requestStatus("GET", "/json/close/${target.targetId}")
        if (dispatched is BrowserTransportResult.Failure) return@withLock dispatched.toOutcome()
        val absent = awaitTargetAbsent(target.targetId)
        if (bound?.binding?.target?.targetId == target.targetId) retireBound()
        ownedTargets.remove(target.targetId)
        handlesByTarget.remove(target.targetId)
        targetsByHandle.remove(handle)
        success(
            code = if (absent) "ok" else "tab_close_postcondition_unverified",
            state = if (absent) CapabilityState.READY else CapabilityState.DEGRADED,
            effect = if (absent) EffectCertainty.VERIFIED else EffectCertainty.MAY_HAVE_OCCURRED,
            snapshotInvalidated = true,
            retryable = !absent,
            data = buildJsonObject {
                put("tab_id", handle)
                put("closed_verified", absent)
            },
        )
    }

    suspend fun retireTurn(turnId: Long): BrowserTurnCleanupReceipt = mutex.withLock {
        val targets = ownedTargets.filterValues {
            it.turnId == turnId && it.lifetime == "turn"
        }.keys.toList()
        var verifiedClosed = 0
        targets.forEach { targetId ->
            val dispatched = transport.requestStatus("GET", "/json/close/$targetId")
            val absent = when (dispatched) {
                is BrowserTransportResult.Ready -> awaitTargetAbsent(targetId)
                is BrowserTransportResult.Failure ->
                    dispatched.effectMayHaveOccurred && awaitTargetAbsent(targetId)
            }
            if (absent) {
                verifiedClosed += 1
                ownedTargets.remove(targetId)
                handlesByTarget.remove(targetId)?.let(targetsByHandle::remove)
                if (bound?.binding?.target?.targetId == targetId) retireBound()
            }
        }
        BrowserTurnCleanupReceipt(
            requested = targets.size,
            verifiedClosed = verifiedClosed,
            unresolved = targets.size - verifiedClosed,
        )
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        cleanupScope.launch {
            try {
                withTimeoutOrNull(SHUTDOWN_TARGET_CLEANUP_MS) {
                    mutex.withLock {
                        ownedTargets.filterValues { it.lifetime == "turn" }
                            .keys
                            .toList()
                            .forEach { targetId ->
                                transport.requestStatus("GET", "/json/close/$targetId")
                            }
                    }
                }
            } finally {
                mutex.withLock {
                    ownedTargets.clear()
                    handlesByTarget.clear()
                    targetsByHandle.clear()
                    retireBound()
                }
                transport.shutdown()
            }
        }
    }

    internal suspend fun freshTargets(): BrowserTransportResult<List<ChromeDevToolsTarget>> =
        when (val result = transport.requestJson("GET", "/json/list")) {
            is BrowserTransportResult.Failure -> result
            is BrowserTransportResult.Ready -> {
                val targets = parseChromeTargets(result.value)
                val liveIds = targets.mapTo(HashSet(), ChromeDevToolsTarget::targetId)
                handlesByTarget.keys.filterNot(liveIds::contains).toList().forEach { id ->
                    handlesByTarget.remove(id)?.let(targetsByHandle::remove)
                    ownedTargets.remove(id)
                }
                bound?.takeIf { it.binding.target.targetId !in liveIds }?.let { retireBound() }
                observationGeneration.incrementAndGet()
                BrowserTransportResult.Ready(targets)
            }
        }

    private suspend fun exactTarget(handle: Int): ExactTargetResult {
        val targetId = targetsByHandle[handle]
            ?: return ExactTargetResult.Failure(targetFailure(handle))
        return when (val targets = freshTargets()) {
            is BrowserTransportResult.Failure ->
                ExactTargetResult.Failure(targets.toOutcome())
            is BrowserTransportResult.Ready -> targets.value
                .singleOrNull { it.targetId == targetId }
                ?.let(ExactTargetResult::Ready)
                ?: ExactTargetResult.Failure(targetFailure(handle))
        }
    }

    internal suspend fun bind(target: ChromeDevToolsTarget): BindResult {
        retireBound()
        val session = when (val opened = ChromeDevToolsSession.open(transport, target.webSocketPath)) {
            is BrowserTransportResult.Failure -> return BindResult.Failure(opened.toOutcome())
            is BrowserTransportResult.Ready -> opened.value
        }
        for (method in listOf("Page.enable", "Runtime.enable")) {
            val enabled = session.send(method)
            if (enabled is CdpCommandResult.Failure) {
                session.close()
                return BindResult.Failure(enabled.toOutcome())
            }
        }
        val frameTree = session.send("Page.getFrameTree")
        if (frameTree is CdpCommandResult.Failure) {
            session.close()
            return BindResult.Failure(frameTree.toOutcome())
        }
        val frame = ((frameTree as CdpCommandResult.Success).result["frameTree"] as? JsonObject)
            ?.get("frame") as? JsonObject
        val generation = observationGeneration.incrementAndGet()
        val loaderId = frame?.get("loaderId")?.jsonPrimitive?.contentOrNull
        val binding = ChromeDeepBinding(
            handle = handleFor(target.targetId),
            target = target,
            documentId = loaderId ?: "${target.targetId}:$generation",
            loaderId = loaderId,
            observationGeneration = generation,
        )
        bound = BoundTarget(binding, session)
        return BindResult.Ready(binding)
    }

    internal fun retireBound() {
        bound?.session?.close()
        bound = null
    }

    private suspend fun awaitTargetAbsent(targetId: String): Boolean {
        repeat(TARGET_CLOSE_ATTEMPTS) {
            val targets = (freshTargets() as? BrowserTransportResult.Ready)?.value ?: return false
            if (targets.none { it.targetId == targetId }) return true
            delay(TARGET_CLOSE_DELAY_MS)
        }
        return false
    }

    private fun handleFor(targetId: String): Int =
        handlesByTarget[targetId] ?: nextHandle.getAndIncrement().also { handle ->
            handlesByTarget[targetId] = handle
            targetsByHandle[handle] = targetId
        }

    private fun parseSingleTarget(element: JsonElement): ChromeDevToolsTarget? =
        parseChromeTargets(JsonArray(listOf(element))).singleOrNull()

    private fun targetFailure(handle: Int): BrowserProviderOutcome = failure(
        "stale_target",
        CapabilityState.DEGRADED,
        "Tab handle $handle is absent from the fresh authenticated target list.",
        retryable = true,
        freshObservationRequired = true,
    )

    internal fun noBoundTarget(): BrowserProviderOutcome = failure(
        "browser_target_not_bound",
        CapabilityState.DEGRADED,
        "Call browser_tabs and browser_switch_tab, or open a tab, before a deep page tool.",
        retryable = true,
        freshObservationRequired = true,
    )

    internal fun success(
        code: String = "ok",
        state: CapabilityState = CapabilityState.READY,
        data: JsonObject = JsonObject(emptyMap()),
        effect: EffectCertainty = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated: Boolean = false,
        retryable: Boolean = false,
    ) = BrowserProviderOutcome(
        code = code,
        state = state,
        providerId = BROWSER_CDP_PROVIDER,
        data = data,
        observationGeneration = observationGeneration.get(),
        effect = effect,
        snapshotInvalidated = snapshotInvalidated,
        retryable = retryable,
    )

    internal fun failure(
        code: String,
        state: CapabilityState,
        message: String,
        effect: EffectCertainty = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated: Boolean = false,
        retryable: Boolean = true,
        requiredUserStep: String? = null,
        freshObservationRequired: Boolean = false,
    ) = BrowserProviderOutcome(
        code = code,
        state = state,
        providerId = BROWSER_CDP_PROVIDER,
        data = buildJsonObject { put("message", message) },
        observationGeneration = observationGeneration.get(),
        effect = effect,
        snapshotInvalidated = snapshotInvalidated,
        retryable = retryable,
        requiredUserStep = requiredUserStep,
        freshObservationRequired = freshObservationRequired,
    )

    internal fun BrowserTransportResult.Failure.toOutcome(): BrowserProviderOutcome = failure(
        code,
        state,
        message,
        effect = if (effectMayHaveOccurred) {
            EffectCertainty.MAY_HAVE_OCCURRED
        } else {
            EffectCertainty.PROVEN_NO_EFFECT
        },
        snapshotInvalidated = effectMayHaveOccurred,
        retryable = retryable,
        requiredUserStep = requiredUserStep,
        freshObservationRequired = effectMayHaveOccurred,
    )

    internal fun CdpCommandResult.Failure.toOutcome(): BrowserProviderOutcome = failure(
        code,
        CapabilityState.DEGRADED,
        message,
        effect = if (effectMayHaveOccurred) {
            EffectCertainty.MAY_HAVE_OCCURRED
        } else {
            EffectCertainty.PROVEN_NO_EFFECT
        },
        snapshotInvalidated = effectMayHaveOccurred,
        retryable = retryable,
        freshObservationRequired = effectMayHaveOccurred,
    )

    internal data class BoundTarget(
        val binding: ChromeDeepBinding,
        val session: ChromeDevToolsSession,
    )

    private data class OwnedTarget(val turnId: Long, val lifetime: String)

    internal sealed interface BindResult {
        data class Ready(val binding: ChromeDeepBinding) : BindResult
        data class Failure(val outcome: BrowserProviderOutcome) : BindResult
    }

    private sealed interface ExactTargetResult {
        data class Ready(val target: ChromeDevToolsTarget) : ExactTargetResult
        data class Failure(val outcome: BrowserProviderOutcome) : ExactTargetResult
    }

    private companion object {
        const val BROWSER_CDP_PROVIDER = "browser_cdp"
        const val TARGET_CLOSE_ATTEMPTS = 10
        const val TARGET_CLOSE_DELAY_MS = 100L
        const val TARGET_DISCOVERY_ATTEMPTS = 20
        const val TARGET_DISCOVERY_DELAY_MS = 100L
        const val MAX_OWNED_TARGETS = 32
        const val SHUTDOWN_TARGET_CLEANUP_MS = 5_000L
        val cleanupScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    }
}

internal sealed interface BrowserTargetBaselineResult {
    data class Ready(val baseline: ChromeTargetBaseline) : BrowserTargetBaselineResult
    data class Failure(val outcome: BrowserProviderOutcome) : BrowserTargetBaselineResult
    data object Unavailable : BrowserTargetBaselineResult
}

internal data class BrowserTurnCleanupReceipt(
    val requested: Int,
    val verifiedClosed: Int,
    val unresolved: Int,
)

internal fun ChromeDeepBinding.toWireJson(): JsonObject = buildJsonObject {
    put("browser_package", "device_local_chromium")
    put("browser_profile_scope", "authenticated_devtools_endpoint")
    put("credential_context_kind", "attached_browser_tab")
    put("tab_id", handle)
    put("target_id_present", true)
    put("document_id", documentId)
    loaderId?.let { put("loader_or_navigation_generation", it) }
    put("observation_generation", observationGeneration)
    put("url", target.url)
    put("title", target.title)
    put("dom_authority", true)
    put("cookie_access", false)
}

internal fun browserHttpUri(raw: String): URI? = runCatching { URI(raw.trim()).normalize() }
    .getOrNull()
    ?.takeIf { uri ->
        uri.scheme?.lowercase() in setOf("http", "https") &&
            !uri.host.isNullOrBlank() &&
            uri.userInfo == null
    }

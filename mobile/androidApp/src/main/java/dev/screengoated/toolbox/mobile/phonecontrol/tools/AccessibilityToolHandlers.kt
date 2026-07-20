package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityActionOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityActionVerb
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityMutationKind
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.surfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal suspend fun handleObserve(
    job: PhoneControlToolJobContext,
    backend: AccessibilityToolBackend = AndroidAccessibilityToolBackend,
): PhoneControlToolExecution = when (val observed = backend.observe()) {
    is AccessibilityProviderResult.Failure -> accessibilityFailure(
        job,
        "observe",
        SEMANTIC_OBSERVE_CAPABILITY,
        observed,
        backend,
    )
    is AccessibilityProviderResult.Success -> PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = "observe",
            capability = SEMANTIC_OBSERVE_CAPABILITY,
            provider = ACCESSIBILITY_PROVIDER,
            providerState = CapabilityState.READY,
            code = "ok",
            observationGeneration = observed.value.observation.generation,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            data = observationData(observed.value),
        ),
        mutating = false,
        refreshScreenFrame = true,
    )
}

internal suspend fun handleAct(
    job: PhoneControlToolJobContext,
    args: JsonObject,
    requestedTool: String = "act",
    backend: AccessibilityToolBackend = AndroidAccessibilityToolBackend,
): PhoneControlToolExecution {
    val id = args.int("id")
        ?: return invalidArgs(job, requestedTool, "$requestedTool requires integer id")
    val verb = parseActionVerb(args.string("verb"))
        ?: return invalidArgs(job, requestedTool, "unknown or missing act verb")
    val confirmed = args.confirmationOrNull()
        ?: return invalidArgs(job, requestedTool, "confirm must be a boolean when provided")
    val capability = capabilityForVerb(verb)
    return when (val outcome = backend.act(id, verb, args.string("value"), confirmed)) {
        is AccessibilityProviderResult.Failure -> accessibilityFailure(
            job,
            requestedTool,
            capability,
            outcome,
            backend,
        )
        is AccessibilityProviderResult.Success -> actionSuccess(
            job,
            requestedTool,
            capability,
            outcome.value,
        )
    }
}

internal suspend fun handleClickAt(
    job: PhoneControlToolJobContext,
    args: JsonObject,
    backend: AccessibilityToolBackend = AndroidAccessibilityToolBackend,
): PhoneControlToolExecution {
    val cell = args.int("cell")
        ?: return invalidArgs(job, "click_at", "click_at requires cell")
    val frame = currentOrFreshFrame(backend)
        ?: return unavailableAccessibility(job, "click_at", POINTER_CAPABILITY)
    val grid = frame.matchingGrid()
        ?: return staleGrid(job, "click_at", backend.observationGeneration)
    val point = grid.cellCenter(cell)
        ?: return invalidArgs(job, "click_at", "grid cell is outside the current frame")
    val lease = grid.surfaceLease
    return gestureExecution(
        job,
        "click_at",
        backend.click(lease, point.first, point.second, grid.visualRevision),
        backend,
    )
}

internal suspend fun handleDrag(
    job: PhoneControlToolJobContext,
    args: JsonObject,
    backend: AccessibilityToolBackend = AndroidAccessibilityToolBackend,
): PhoneControlToolExecution {
    val from = args.int("from_cell")
        ?: return invalidArgs(job, "drag", "drag requires from_cell")
    val to = args.int("to_cell")
        ?: return invalidArgs(job, "drag", "drag requires to_cell")
    val frame = currentOrFreshFrame(backend)
        ?: return unavailableAccessibility(job, "drag", POINTER_CAPABILITY)
    val grid = frame.matchingGrid()
        ?: return staleGrid(job, "drag", backend.observationGeneration)
    val fromPoint = grid.cellCenter(from)
        ?: return invalidArgs(job, "drag", "from_cell is outside the current frame")
    val toPoint = grid.cellCenter(to)
        ?: return invalidArgs(job, "drag", "to_cell is outside the current frame")
    val lease = grid.surfaceLease
    return gestureExecution(
        job,
        "drag",
        backend.swipe(
            lease,
            fromPoint.first,
            fromPoint.second,
            toPoint.first,
            toPoint.second,
            550L,
            AccessibilityMutationKind.POINTER_ACTIVATE,
            grid.visualRevision,
        ),
        backend,
    )
}

internal suspend fun handleScroll(
    job: PhoneControlToolJobContext,
    args: JsonObject,
    backend: AccessibilityToolBackend = AndroidAccessibilityToolBackend,
): PhoneControlToolExecution {
    val direction = args.string("direction")
        ?: return invalidArgs(job, "scroll", "scroll requires direction")
    val frame = currentOrFreshFrame(backend)
        ?: return unavailableAccessibility(job, "scroll", POINTER_CAPABILITY)
    val requestedCell = args.int("cell")
    val grid = frame.matchingGrid()
    val lease = if (grid != null) {
        grid.surfaceLease
    } else {
        val candidates = frame.observation.windows.filter { window ->
            window.active && window.focused && !window.controllerOwned
        }
        candidates.singleOrNull()?.surfaceLease(frame.observation.generation)
            ?: return unavailableAccessibility(job, "scroll", POINTER_CAPABILITY)
    }
    val bounds = lease.bounds
    val center = if (requestedCell == null) {
        (bounds.left + bounds.right) / 2f to (bounds.top + bounds.bottom) / 2f
    } else {
        if (grid == null) return staleGrid(job, "scroll", backend.observationGeneration)
        grid.cellCenter(requestedCell)
            ?: return invalidArgs(job, "scroll", "grid cell is outside the current frame")
    }
    val amount = args.number("amount")?.coerceIn(0.25, 10.0) ?: 1.0
    val horizontal = (bounds.right - bounds.left) *
        (0.25f * amount.toFloat()).coerceAtMost(0.42f)
    val vertical = (bounds.bottom - bounds.top) *
        (0.25f * amount.toFloat()).coerceAtMost(0.42f)
    val coordinates = scrollCoordinates(direction, center, bounds, horizontal, vertical)
        ?: return invalidArgs(job, "scroll", "unknown scroll direction")
    return gestureExecution(
        job,
        "scroll",
        backend.swipe(
            lease,
            coordinates[0],
            coordinates[1],
            coordinates[2],
            coordinates[3],
            420L,
            AccessibilityMutationKind.NAVIGATION_GESTURE,
            grid?.visualRevision.takeIf { requestedCell != null },
        ),
        backend,
    )
}

private fun AccessibilityObservationFrame.matchingGrid(): AccessibilityGridIdentity? =
    grid?.takeIf { it.matches(observation) }

private fun actionSuccess(
    job: PhoneControlToolJobContext,
    requestedTool: String,
    capability: String,
    outcome: AccessibilityActionOutcome,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = requestedTool,
        capability = capability,
        provider = ACCESSIBILITY_PROVIDER,
        providerState = CapabilityState.READY,
        code = outcome.code,
        observationGeneration = outcome.generation,
        effect = outcome.effect,
        snapshotInvalidated = outcome.snapshotInvalidated,
        freshObservationRequired = outcome.freshObservationRequired,
        data = buildJsonObject { outcome.message?.let { put("message", it) } },
    ),
    mutating = outcome.effect != EffectCertainty.PROVEN_NO_EFFECT,
    refreshScreenFrame = outcome.snapshotInvalidated,
)

private fun gestureExecution(
    job: PhoneControlToolJobContext,
    tool: String,
    result: AccessibilityProviderResult<AccessibilityGestureOutcome>,
    backend: AccessibilityToolBackend,
): PhoneControlToolExecution = when (result) {
    is AccessibilityProviderResult.Failure -> accessibilityFailure(
        job,
        tool,
        POINTER_CAPABILITY,
        result,
        backend,
    )
    is AccessibilityProviderResult.Success -> PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = tool,
            capability = POINTER_CAPABILITY,
            provider = ACCESSIBILITY_PROVIDER,
            providerState = CapabilityState.READY,
            code = result.value.code,
            observationGeneration = result.value.generation,
            effect = result.value.effect,
            snapshotInvalidated = result.value.snapshotInvalidated,
            freshObservationRequired = result.value.snapshotInvalidated,
        ),
        mutating = result.value.effect != EffectCertainty.PROVEN_NO_EFFECT,
        refreshScreenFrame = result.value.snapshotInvalidated,
    )
}

private fun staleSurfaceExecution(
    job: PhoneControlToolJobContext,
    tool: String,
    backend: AccessibilityToolBackend,
): PhoneControlToolExecution = gestureExecution(
    job,
    tool,
    AccessibilityProviderResult.Failure(
        code = "stale_target",
        message = "The visual surface no longer belongs to the current observation.",
        retryable = true,
        freshObservationRequired = true,
    ),
    backend,
)

private suspend fun currentOrFreshFrame(
    backend: AccessibilityToolBackend,
): AccessibilityObservationFrame? =
    (backend.observe() as? AccessibilityProviderResult.Success)?.value

private fun scrollCoordinates(
    direction: String,
    center: Pair<Float, Float>,
    bounds: TargetBounds,
    horizontal: Float,
    vertical: Float,
): List<Float>? {
    fun x(value: Float) = value.coerceIn(bounds.left + 1f, bounds.right - 1f)
    fun y(value: Float) = value.coerceIn(bounds.top + 1f, bounds.bottom - 1f)
    val centerX = x(center.first)
    val centerY = y(center.second)
    return when (direction) {
        "down" -> listOf(centerX, y(centerY + vertical), centerX, y(centerY - vertical))
        "up" -> listOf(centerX, y(centerY - vertical), centerX, y(centerY + vertical))
        "right" -> listOf(x(centerX + horizontal), centerY, x(centerX - horizontal), centerY)
        "left" -> listOf(x(centerX - horizontal), centerY, x(centerX + horizontal), centerY)
        else -> null
    }
}

internal fun observationData(frame: AccessibilityObservationFrame): JsonObject = buildJsonObject {
    val observation = frame.observation
    semanticObservationData(observation).forEach { (key, value) -> put(key, value) }
    frame.matchingGrid()?.let { grid ->
        put(
            "grid",
            buildJsonObject {
                put("identity", grid.wireIdentity)
                put("observation_generation", grid.observationGeneration)
                put("display_id", grid.displayId)
                put("window_id", grid.windowId)
                put("columns", grid.columns)
                put("rows", grid.rows)
            },
        )
    }
}

internal fun semanticObservationData(
    observation: AccessibilityObservation,
): JsonObject = buildJsonObject {
    val context = SurfaceContext(observation, observation.surfaceLease())
    put("state_reconciled", true)
    put("elements", observation.toModelText())
    put("element_count", observation.elements.size)
    put("window_count", observation.windows.size)
    put("truncated", observation.truncated)
    put("surface_targets", buildJsonArray {
        observation.windows
            .asSequence()
            .filter { window -> window.systemNavigationTargetable() }
            .sortedWith(compareBy({ it.displayId }, { it.layer }, { it.id }))
            .forEach { window ->
                val descriptor = context.descriptor(window) ?: return@forEach
                add(buildJsonObject {
                    put("target", descriptor.target)
                    put("display_id", window.displayId)
                    put("window_id", window.id)
                    put("package", window.packageName.orEmpty())
                    put("title", window.title.orEmpty())
                    put("active", window.active)
                    put("focused", window.focused)
                })
            }
    })
}

private fun accessibilityFailure(
    job: PhoneControlToolJobContext,
    tool: String,
    capability: String,
    failure: AccessibilityProviderResult.Failure,
    backend: AccessibilityToolBackend,
): PhoneControlToolExecution {
    val uncertain = failure.effect != EffectCertainty.PROVEN_NO_EFFECT
    val providerState = if (backend.isReady) {
        if (failure.requiredUserStep != null) CapabilityState.READY else CapabilityState.DEGRADED
    } else {
        CapabilityState.NEEDS_USER_STEP
    }
    return PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = tool,
            capability = capability,
            provider = ACCESSIBILITY_PROVIDER,
            providerState = providerState,
            code = failure.code,
            observationGeneration = backend.observationGeneration,
            effect = failure.effect,
            snapshotInvalidated = uncertain,
            retryable = failure.retryable,
            requiredUserStep = failure.requiredUserStep
                ?: if (backend.isReady) null else "enable_accessibility",
            freshObservationRequired = failure.freshObservationRequired || uncertain,
            data = buildJsonObject {
                put("message", failure.message)
            },
        ),
        mutating = uncertain,
        refreshScreenFrame = uncertain,
    )
}

internal fun invalidArgs(
    job: PhoneControlToolJobContext,
    tool: String,
    message: String,
    observationGeneration: Long = 0,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = tool,
        capability = "tool_contract",
        provider = "android_app_api",
        providerState = CapabilityState.READY,
        code = "invalid_arguments",
        observationGeneration = observationGeneration,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        data = buildJsonObject { put("message", message) },
    ),
    mutating = false,
)

private fun unavailableAccessibility(
    job: PhoneControlToolJobContext,
    tool: String,
    capability: String,
): PhoneControlToolExecution = unavailableToolResponse(
    job,
    tool,
    capability,
    ACCESSIBILITY_PROVIDER,
    CapabilityState.NEEDS_USER_STEP,
    "enable_accessibility",
)

private fun staleGrid(
    job: PhoneControlToolJobContext,
    tool: String,
    observationGeneration: Long,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = tool,
        capability = POINTER_CAPABILITY,
        provider = ACCESSIBILITY_PROVIDER,
        providerState = CapabilityState.READY,
        code = "stale_frame",
        observationGeneration = observationGeneration,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = true,
        freshObservationRequired = true,
        data = buildJsonObject {
            put(
                "message",
                "The numbered frame is not bound to the current Accessibility surface.",
            )
        },
    ),
    mutating = false,
    refreshScreenFrame = true,
)

internal fun capabilityForVerb(verb: AccessibilityActionVerb): String =
    if (verb == AccessibilityActionVerb.FILL) TEXT_EDIT_CAPABILITY else POINTER_CAPABILITY

private fun parseActionVerb(value: String?): AccessibilityActionVerb? = value
    ?.uppercase()
    ?.let { name -> runCatching { AccessibilityActionVerb.valueOf(name) }.getOrNull() }

private const val ACCESSIBILITY_PROVIDER = "accessibility"
private const val SEMANTIC_OBSERVE_CAPABILITY = "ui.semantic_observe"
private const val POINTER_CAPABILITY = "ui.pointer_action"
private const val TEXT_EDIT_CAPABILITY = "ui.text_edit"

package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.accessibilityservice.AccessibilityService
import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AndroidSurfaceDescriptor
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AndroidSurfaceResolution
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AndroidSurfaceResolutionError
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.surfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonObjectBuilder
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class SurfaceToolHandlers(
    private val backend: SurfaceToolBackend,
) {
    constructor(context: Context) : this(AndroidSurfaceToolBackend(context))

    suspend fun listWindows(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        listSurfaces(job, "list_windows", APP_TASK_CAPABILITY, activeOnly = false)

    suspend fun queryWindows(
        job: PhoneControlToolJobContext,
        query: String,
    ): PhoneControlToolExecution = when (query) {
        "list" -> listSurfaces(job, "system_query", SYSTEM_QUERY_CAPABILITY, activeOnly = false)
        "active" -> listSurfaces(job, "system_query", SYSTEM_QUERY_CAPABILITY, activeOnly = true)
        else -> invalidArgs(job, "system_query", "Unsupported window query.")
    }

    suspend fun focusWindow(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val requested = args.string("title")?.takeIf(String::isNotBlank)
            ?: return invalidArgs(job, "focus_window", "focus_window requires title")
        val context = when (val observed = currentSurfaceContext()) {
            is SurfaceContextResult.Failure -> return providerFailure(job, "focus_window", observed.failure)
            is SurfaceContextResult.Success -> observed.context
        }
        val surface = when (val resolved = context.lease.resolve(requested)) {
            is AndroidSurfaceResolution.Rejected -> {
                return resolutionFailure(job, "focus_window", context.observation.generation, resolved.error)
            }
            is AndroidSurfaceResolution.Resolved -> resolved.surface
        }
        val snapshot = context.snapshot(surface)
            ?: return staleSurface(job, "focus_window", context.observation.generation)
        if (!snapshot.focusTargetable()) {
            return unsupportedSurface(job, "focus_window", context.observation.generation)
        }
        if (snapshot.active && snapshot.focused) {
            return surfaceSuccess(
                job = job,
                tool = "focus_window",
                generation = context.observation.generation,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                data = buildJsonObject { putSurface("surface", surface, snapshot) },
            )
        }
        if (!backend.isPackageLaunchable(snapshot.packageName.orEmpty())) {
            return unsupportedSurface(
                job,
                "focus_window",
                context.observation.generation,
                provider = APP_PROVIDER,
            )
        }
        when (val launched = backend.launchPackage(snapshot.packageName.orEmpty())) {
            is AndroidProviderResult.Failure -> {
                return appDispatchFailure(job, "focus_window", context.observation.generation, launched)
            }
            is AndroidProviderResult.Success -> backend.invalidate("focus_window_dispatch")
        }
        var ambiguousPostcondition: AmbiguousFocusPostcondition? = null
        repeat(POSTCONDITION_ATTEMPTS) {
            backend.postconditionPause()
            val fresh = (currentSurfaceContext() as? SurfaceContextResult.Success)?.context
                ?: return@repeat
            when (val resolved = fresh.resolveFocusedContinuation(snapshot)) {
                FocusedContinuation.NotFound -> Unit
                is FocusedContinuation.Ambiguous -> {
                    ambiguousPostcondition = AmbiguousFocusPostcondition(
                        fresh.observation.generation,
                        resolved.choices,
                    )
                }
                is FocusedContinuation.Resolved -> {
                    val descriptor = fresh.descriptor(resolved.window) ?: return@repeat
                    return surfaceSuccess(
                        job = job,
                        tool = "focus_window",
                        generation = fresh.observation.generation,
                        effect = EffectCertainty.VERIFIED,
                        snapshotInvalidated = true,
                        data = buildJsonObject { putSurface("surface", descriptor, resolved.window) },
                        provider = APP_PROVIDER,
                    )
                }
            }
        }
        ambiguousPostcondition?.let { ambiguous ->
            return uncertainSurfaceFailure(
                job = job,
                tool = "focus_window",
                code = "focus_ambiguous_postcondition",
                message = "The launch was dispatched, but more than one fresh surface matched it.",
                generation = ambiguous.generation,
                data = buildJsonObject {
                    put("choices", JsonArray(ambiguous.choices.map { JsonPrimitive(it.target) }))
                },
                provider = APP_PROVIDER,
            )
        }
        return uncertainSurfaceFailure(
            job,
            "focus_window",
            "focus_not_verified",
            "The app launch was dispatched, but foreground focus was not verified.",
            provider = APP_PROVIDER,
        )
    }

    suspend fun minimizeWindow(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val requested = args.string("title")?.takeIf(String::isNotBlank)
            ?: return invalidArgs(job, "minimize_window", "minimize_window requires title")
        val context = when (val observed = currentSurfaceContext()) {
            is SurfaceContextResult.Failure -> return providerFailure(job, "minimize_window", observed.failure)
            is SurfaceContextResult.Success -> observed.context
        }
        val surface = when (val resolved = context.lease.resolve(requested)) {
            is AndroidSurfaceResolution.Rejected -> {
                return resolutionFailure(job, "minimize_window", context.observation.generation, resolved.error)
            }
            is AndroidSurfaceResolution.Resolved -> resolved.surface
        }
        val snapshot = context.snapshot(surface)
            ?: return staleSurface(job, "minimize_window", context.observation.generation)
        val displayBounds = backend.displayBounds(snapshot.displayId)
        if (displayBounds == null || !context.canMinimize(snapshot, displayBounds)) {
            return unsupportedSurface(job, "minimize_window", context.observation.generation)
        }
        val mutationLease = snapshot.surfaceLease(context.observation.generation)
            ?: return staleSurface(job, "minimize_window", context.observation.generation)
        when (val action = backend.globalAction(
            mutationLease,
            AccessibilityService.GLOBAL_ACTION_HOME,
        )) {
            is AccessibilityProviderResult.Failure -> {
                return providerFailure(job, "minimize_window", action)
            }
            is AccessibilityProviderResult.Success -> if (action.value.code != "ok") {
                return gestureNoEffect(job, "minimize_window", action.value)
            }
        }
        repeat(POSTCONDITION_ATTEMPTS) {
            backend.postconditionPause()
            val fresh = (currentSurfaceContext() as? SurfaceContextResult.Success)?.context
                ?: return@repeat
            val stillForeground = fresh.observation.windows.any { candidate ->
                candidate.packageName == snapshot.packageName &&
                    candidate.type == APPLICATION_WINDOW &&
                    (candidate.active || candidate.focused)
            }
            if (!stillForeground) {
                val foreground = fresh.observation.windows.firstOrNull { candidate ->
                    candidate.displayId == snapshot.displayId &&
                        candidate.type == APPLICATION_WINDOW && candidate.active && candidate.focused
                }
                return surfaceSuccess(
                    job = job,
                    tool = "minimize_window",
                    generation = fresh.observation.generation,
                    effect = EffectCertainty.VERIFIED,
                    snapshotInvalidated = true,
                    data = buildJsonObject {
                        foreground?.let { current ->
                            fresh.descriptor(current)?.let { putSurface("foreground", it, current) }
                        }
                    },
                )
            }
        }
        return uncertainSurfaceFailure(
            job,
            "minimize_window",
            "minimize_not_verified",
            "Home was dispatched, but the requested surface leaving the foreground was not verified.",
        )
    }

    fun unsupportedGeometry(
        job: PhoneControlToolJobContext,
        tool: String,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val title = args.string("title")?.takeIf(String::isNotBlank)
            ?: return invalidArgs(job, tool, "$tool requires title")
        if (tool == "resize_window") {
            val width = args.int("width") ?: return invalidArgs(job, tool, "$tool requires width")
            val height = args.int("height") ?: return invalidArgs(job, tool, "$tool requires height")
            if (width <= 0 || height <= 0) return invalidArgs(job, tool, "Window size must be positive.")
        } else {
            args.int("x") ?: return invalidArgs(job, tool, "$tool requires x")
            args.int("y") ?: return invalidArgs(job, tool, "$tool requires y")
        }
        return PhoneControlToolExecution(
            response = toolResponse(
                job = job,
                requestedTool = tool,
                capability = APP_TASK_CAPABILITY,
                provider = PRIVILEGED_PROVIDER,
                providerState = CapabilityState.UNSUPPORTED,
                code = "unsupported_on_surface",
                observationGeneration = backend.observationGeneration,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                data = buildJsonObject {
                    put("target", title)
                    put("message", "Arbitrary third-party Android surface geometry is unavailable.")
                },
            ),
            mutating = false,
        )
    }

    private suspend fun listSurfaces(
        job: PhoneControlToolJobContext,
        tool: String,
        capability: String,
        activeOnly: Boolean,
    ): PhoneControlToolExecution = when (val observed = currentSurfaceContext()) {
        is SurfaceContextResult.Failure -> providerFailure(job, tool, observed.failure, capability)
        is SurfaceContextResult.Success -> {
            val context = observed.context
            val windows = if (activeOnly) {
                context.observation.windows.filter { it.active || it.focused }
            } else {
                context.observation.windows
            }
            surfaceSuccess(
                job = job,
                tool = tool,
                capability = capability,
                generation = context.observation.generation,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                data = buildJsonObject {
                    put("visibility_scope", "current_interactive_surfaces_only")
                    put("windows", buildJsonArray {
                        windows.forEach { snapshot ->
                            add(context.windowJson(snapshot, backend.appLabel(snapshot.packageName.orEmpty())))
                        }
                    })
                },
            )
        }
    }

    private suspend fun currentSurfaceContext(): SurfaceContextResult = when (val observed = backend.observe()) {
        is AccessibilityProviderResult.Failure -> SurfaceContextResult.Failure(observed)
        is AccessibilityProviderResult.Success -> SurfaceContextResult.Success(
            SurfaceContext(observed.value, observed.value.surfaceLease()),
        )
    }

    private fun surfaceSuccess(
        job: PhoneControlToolJobContext,
        tool: String,
        generation: Long,
        effect: EffectCertainty,
        snapshotInvalidated: Boolean,
        data: JsonObject,
        capability: String = APP_TASK_CAPABILITY,
        provider: String = ACCESSIBILITY_PROVIDER,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = tool,
            capability = capability,
            provider = provider,
            providerState = CapabilityState.READY,
            code = "ok",
            observationGeneration = generation,
            effect = effect,
            snapshotInvalidated = snapshotInvalidated,
            freshObservationRequired = snapshotInvalidated.takeIf { it },
            data = data,
        ),
        mutating = effect.effectMayHaveOccurred == true,
        refreshScreenFrame = snapshotInvalidated,
    )

    private fun providerFailure(
        job: PhoneControlToolJobContext,
        tool: String,
        failure: AccessibilityProviderResult.Failure,
        capability: String = APP_TASK_CAPABILITY,
    ): PhoneControlToolExecution {
        val providerState = when {
            !backend.isReady -> CapabilityState.NEEDS_USER_STEP
            failure.requiredUserStep != null -> CapabilityState.READY
            else -> CapabilityState.DEGRADED
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
                snapshotInvalidated = failure.effect != EffectCertainty.PROVEN_NO_EFFECT,
                retryable = failure.retryable,
                requiredUserStep = failure.requiredUserStep
                    ?: if (backend.isReady) null else "enable_accessibility",
                freshObservationRequired = failure.freshObservationRequired ||
                    failure.effect != EffectCertainty.PROVEN_NO_EFFECT,
                data = buildJsonObject {
                    put("message", failure.message)
                },
            ),
            mutating = failure.effect != EffectCertainty.PROVEN_NO_EFFECT,
            refreshScreenFrame = failure.effect != EffectCertainty.PROVEN_NO_EFFECT,
        )
    }

    private fun resolutionFailure(
        job: PhoneControlToolJobContext,
        tool: String,
        generation: Long,
        error: AndroidSurfaceResolutionError,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = tool,
            capability = APP_TASK_CAPABILITY,
            provider = ACCESSIBILITY_PROVIDER,
            providerState = CapabilityState.DEGRADED,
            code = error.code,
            observationGeneration = generation,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            retryable = error.code == "stale_target",
            freshObservationRequired = (error.code == "stale_target").takeIf { it },
            data = buildJsonObject {
                put("message", error.message())
                if (error is AndroidSurfaceResolutionError.Ambiguous) {
                    put("choices", JsonArray(error.choices.map(::JsonPrimitive)))
                }
            },
        ),
        mutating = false,
    )

    private fun staleSurface(
        job: PhoneControlToolJobContext,
        tool: String,
        generation: Long,
    ): PhoneControlToolExecution = resolutionFailure(
        job,
        tool,
        generation,
        AndroidSurfaceResolutionError.StableTargetNotFound(""),
    )

    private fun unsupportedSurface(
        job: PhoneControlToolJobContext,
        tool: String,
        generation: Long,
        provider: String = ACCESSIBILITY_PROVIDER,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = tool,
            capability = APP_TASK_CAPABILITY,
            provider = provider,
            providerState = CapabilityState.UNSUPPORTED,
            code = "unsupported_on_surface",
            observationGeneration = generation,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
        ),
        mutating = false,
    )

    private fun appDispatchFailure(
        job: PhoneControlToolJobContext,
        tool: String,
        generation: Long,
        failure: AndroidProviderResult.Failure,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = tool,
            capability = APP_TASK_CAPABILITY,
            provider = APP_PROVIDER,
            providerState = CapabilityState.READY,
            code = failure.code,
            observationGeneration = generation,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            retryable = failure.retryable,
            requiredUserStep = failure.requiredUserStep,
            data = buildJsonObject { put("message", failure.message) },
        ),
        mutating = false,
    )

    private fun gestureNoEffect(
        job: PhoneControlToolJobContext,
        tool: String,
        outcome: AccessibilityGestureOutcome,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = tool,
            capability = APP_TASK_CAPABILITY,
            provider = ACCESSIBILITY_PROVIDER,
            providerState = CapabilityState.READY,
            code = outcome.code,
            observationGeneration = outcome.generation,
            effect = outcome.effect,
            snapshotInvalidated = outcome.snapshotInvalidated,
        ),
        mutating = false,
    )

    private fun uncertainSurfaceFailure(
        job: PhoneControlToolJobContext,
        tool: String,
        code: String,
        message: String,
        generation: Long = backend.observationGeneration,
        data: JsonObject = buildJsonObject { put("message", message) },
        provider: String = ACCESSIBILITY_PROVIDER,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = tool,
            capability = APP_TASK_CAPABILITY,
            provider = provider,
            providerState = CapabilityState.READY,
            code = code,
            observationGeneration = generation,
            effect = EffectCertainty.MAY_HAVE_OCCURRED,
            snapshotInvalidated = true,
            retryable = true,
            freshObservationRequired = true,
            data = JsonObject(data + ("message" to JsonPrimitive(message))),
        ),
        mutating = true,
        refreshScreenFrame = true,
    )
}

private sealed interface SurfaceContextResult {
    data class Success(val context: SurfaceContext) : SurfaceContextResult
    data class Failure(val failure: AccessibilityProviderResult.Failure) : SurfaceContextResult
}

private data class AmbiguousFocusPostcondition(
    val generation: Long,
    val choices: List<AndroidSurfaceDescriptor>,
)

private fun JsonObjectBuilder.putSurface(
    name: String,
    descriptor: AndroidSurfaceDescriptor,
    snapshot: AccessibilityWindowSnapshot,
) {
    put(name, buildJsonObject {
        put("target", descriptor.target)
        put("package", descriptor.identity.packageName)
        put("title", descriptor.title.orEmpty())
        put("display_id", descriptor.identity.displayId)
        put("window_id", descriptor.identity.windowId)
        put("active", snapshot.active)
        put("focused", snapshot.focused)
    })
}

private fun AndroidSurfaceResolutionError.message(): String = when (this) {
    AndroidSurfaceResolutionError.EmptyTarget -> "The surface target is empty."
    is AndroidSurfaceResolutionError.MalformedStableTarget -> "The stable surface target is malformed."
    is AndroidSurfaceResolutionError.StaleGeneration -> "The surface target belongs to a stale observation."
    is AndroidSurfaceResolutionError.WrongPackage -> "The window identity now belongs to another package."
    is AndroidSurfaceResolutionError.WrongDisplay -> "The window identity now belongs to another display."
    is AndroidSurfaceResolutionError.ReusedWindowId -> "The Android window ID was reused."
    is AndroidSurfaceResolutionError.StableTargetNotFound -> "The stable surface target no longer exists."
    is AndroidSurfaceResolutionError.NamedTargetNotFound -> "No current surface exactly matches the request."
    is AndroidSurfaceResolutionError.Ambiguous -> "More than one current surface exactly matches the request."
}

private const val APP_TASK_CAPABILITY = "app_and_task_control"
private const val SYSTEM_QUERY_CAPABILITY = "system_query"
private const val ACCESSIBILITY_PROVIDER = "accessibility"
private const val APP_PROVIDER = "android_app_api"
private const val PRIVILEGED_PROVIDER = "privileged_system"
private const val POSTCONDITION_ATTEMPTS = 20

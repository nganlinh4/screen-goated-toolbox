package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.accessibilityservice.AccessibilityService
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityElement
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.surfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal enum class AndroidSystemNavigationKey(
    val wireName: String,
    val globalAction: Int,
) {
    BACK("back", AccessibilityService.GLOBAL_ACTION_BACK),
    HOME("home", AccessibilityService.GLOBAL_ACTION_HOME),
    RECENTS("recents", AccessibilityService.GLOBAL_ACTION_RECENTS),
    NOTIFICATIONS("notifications", AccessibilityService.GLOBAL_ACTION_NOTIFICATIONS),
    QUICK_SETTINGS("quick_settings", AccessibilityService.GLOBAL_ACTION_QUICK_SETTINGS),
}

internal fun parseAndroidSystemNavigationKey(raw: String?): AndroidSystemNavigationKey? {
    val key = raw?.trim()?.lowercase() ?: return null
    return AndroidSystemNavigationKey.entries.singleOrNull { it.wireName == key }
}

internal class AndroidSystemNavigationToolHandler(
    private val backend: SurfaceToolBackend,
) {
    suspend fun execute(
        job: PhoneControlToolJobContext,
        args: JsonObject,
        key: AndroidSystemNavigationKey,
    ): PhoneControlToolExecution {
        val targetIdentity = parseAndroidTextTarget(args.string("target"))
            ?: return invalidArgs(job, TOOL, TARGET_MESSAGE)
        val holdSeconds = args.number("hold_seconds") ?: if ("hold_seconds" in args) {
            return invalidArgs(job, TOOL, "hold_seconds must be numeric")
        } else {
            0.0
        }
        if (!holdSeconds.isFinite() || holdSeconds != 0.0) {
            return invalidArgs(job, TOOL, "Android system navigation keys do not support holding")
        }

        val before = when (val observed = backend.observe()) {
            is AccessibilityProviderResult.Failure -> return failure(job, observed)
            is AccessibilityProviderResult.Success -> observed.value
        }
        val surfaceContext = SurfaceContext(before, before.surfaceLease())
        val window = surfaceContext.systemNavigationSnapshot(targetIdentity)
            ?: return stale(job, before.generation)
        if (!window.systemNavigationTargetable()) {
            return noEffect(
                job = job,
                code = "target_not_foreground",
                generation = before.generation,
                retryable = true,
                message = "System navigation requires the current foreground surface token.",
            )
        }
        val lease = window.surfaceLease(before.generation)
            ?: return stale(job, before.generation)
        val outcome = when (val dispatched = backend.globalAction(lease, key.globalAction)) {
            is AccessibilityProviderResult.Failure -> return failure(job, dispatched)
            is AccessibilityProviderResult.Success -> dispatched.value
        }
        if (outcome.effect == EffectCertainty.PROVEN_NO_EFFECT) {
            return noEffect(
                job = job,
                code = outcome.code,
                generation = outcome.generation,
                retryable = false,
                message = outcome.message,
            )
        }

        backend.postconditionPause()
        val after = when (val observed = backend.observe()) {
            is AccessibilityProviderResult.Failure -> {
                return uncertain(job, outcome.code, outcome.generation, key, observed.message)
            }
            is AccessibilityProviderResult.Success -> observed.value
        }
        val verified = changedForNavigation(before, after)
        return PhoneControlToolExecution(
            response = toolResponse(
                job = job,
                requestedTool = TOOL,
                capability = CAPABILITY,
                provider = PROVIDER,
                providerState = CapabilityState.READY,
                code = if (verified) "ok" else "navigation_not_verified",
                observationGeneration = after.generation,
                effect = if (verified) EffectCertainty.VERIFIED else EffectCertainty.MAY_HAVE_OCCURRED,
                snapshotInvalidated = true,
                retryable = !verified,
                freshObservationRequired = false,
                data = buildJsonObject {
                    put("keys", key.wireName)
                    put("fresh_observation_attached", true)
                    semanticObservationData(after).forEach { (name, value) -> put(name, value) }
                },
            ),
            mutating = true,
            refreshScreenFrame = true,
        )
    }

    private fun failure(
        job: PhoneControlToolJobContext,
        failure: AccessibilityProviderResult.Failure,
    ): PhoneControlToolExecution {
        val uncertain = failure.effect != EffectCertainty.PROVEN_NO_EFFECT
        return PhoneControlToolExecution(
            response = toolResponse(
                job = job,
                requestedTool = TOOL,
                capability = CAPABILITY,
                provider = PROVIDER,
                providerState = if (backend.isReady) {
                    CapabilityState.DEGRADED
                } else {
                    CapabilityState.NEEDS_USER_STEP
                },
                code = failure.code,
                observationGeneration = backend.observationGeneration,
                effect = failure.effect,
                snapshotInvalidated = uncertain,
                retryable = failure.retryable,
                requiredUserStep = failure.requiredUserStep
                    ?: if (backend.isReady) null else "enable_accessibility",
                freshObservationRequired = failure.freshObservationRequired || uncertain,
                data = buildJsonObject { put("message", failure.message) },
            ),
            mutating = uncertain,
            refreshScreenFrame = uncertain,
        )
    }

    private fun stale(
        job: PhoneControlToolJobContext,
        generation: Long,
    ): PhoneControlToolExecution = noEffect(
        job,
        "stale_target",
        generation,
        retryable = true,
        message = "The surface token does not belong to the current observation.",
        freshObservationRequired = true,
    )

    private fun noEffect(
        job: PhoneControlToolJobContext,
        code: String,
        generation: Long,
        retryable: Boolean,
        message: String?,
        freshObservationRequired: Boolean = false,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = TOOL,
            capability = CAPABILITY,
            provider = PROVIDER,
            providerState = CapabilityState.READY,
            code = code,
            observationGeneration = generation,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            retryable = retryable,
            freshObservationRequired = freshObservationRequired,
            data = buildJsonObject { message?.let { put("message", it) } },
        ),
        mutating = false,
    )

    private fun uncertain(
        job: PhoneControlToolJobContext,
        code: String,
        generation: Long,
        key: AndroidSystemNavigationKey,
        message: String,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = TOOL,
            capability = CAPABILITY,
            provider = PROVIDER,
            providerState = CapabilityState.DEGRADED,
            code = code,
            observationGeneration = generation,
            effect = EffectCertainty.MAY_HAVE_OCCURRED,
            snapshotInvalidated = true,
            retryable = true,
            freshObservationRequired = true,
            data = buildJsonObject {
                put("keys", key.wireName)
                put("message", message)
            },
        ),
        mutating = true,
        refreshScreenFrame = true,
    )
}

private fun changedForNavigation(
    before: AccessibilityObservation,
    after: AccessibilityObservation,
): Boolean = before.displayRotation != after.displayRotation ||
    before.windows != after.windows ||
    before.elements.map(AccessibilityElement::semanticState) !=
    after.elements.map(AccessibilityElement::semanticState)

private fun AccessibilityElement.semanticState(): List<Any?> = listOf(
    role, label, value, hint, stateDescription, viewId, packageName, className, bounds,
    actions, enabled, visible, focused, selected, checked, isProtected, controllerOwned,
    targetAuthority,
)

private const val TOOL = "key_combination"
private const val CAPABILITY = "ui.key_action"
private const val PROVIDER = "accessibility"
private const val TARGET_MESSAGE =
    "Android key actions require a current stable surface target from list_windows"

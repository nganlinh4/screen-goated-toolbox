package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityElement
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import java.net.URI
import java.util.UUID
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.add
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal data class BrowserSurfaceBinding(
    val surfaceId: String,
    val observationGeneration: Long,
    val displayId: Int,
    val windowId: Long,
    val packageName: String,
    val bounds: TargetBounds,
    val observationTimestampMs: Long,
    val windowTitle: String?,
    val credentialContextKind: String,
    val launchedByPhoneControl: Boolean,
) {
    fun toWireJson(): JsonObject = buildJsonObject {
        put("surface_identity", surfaceId)
        put("snapshot_generation", observationGeneration)
        put("observation_generation", observationGeneration)
        put("display_id", displayId)
        put("window_id", windowId)
        put("browser_package", packageName)
        put("package_or_surface", packageName)
        put("node_or_document_identity", surfaceId)
        put("bounds", bounds.toWireJson())
        put("observation_timestamp", observationTimestampMs)
        put("credential_context_kind", credentialContextKind)
        put("control_provider", "accessibility")
        put("deep_target_bound", false)
        put("dom_authority", false)
        put("cookie_access", false)
        put("launched_by_phone_control", launchedByPhoneControl)
    }
}

internal data class BrowserSurfaceSnapshot(
    val binding: BrowserSurfaceBinding,
    val observation: AccessibilityObservation,
    val elements: List<AccessibilityElement>,
    val observedUrl: String?,
)

internal sealed interface BrowserSurfaceResolution {
    data class Success(val snapshot: BrowserSurfaceSnapshot) : BrowserSurfaceResolution

    data class Failure(
        val code: String,
        val message: String,
        val retryable: Boolean,
        val kind: BrowserSurfaceFailureKind,
        val observationGeneration: Long,
        val providerId: String? = null,
        val providerState: CapabilityState = CapabilityState.DEGRADED,
        val requiredUserStep: String? = null,
        val freshObservationRequired: Boolean = false,
    ) : BrowserSurfaceResolution {
        init {
            require(code.isNotBlank() && code != "ok")
            require(observationGeneration >= 0)
            require(
                (kind == BrowserSurfaceFailureKind.SURFACE_STATE && providerId == null) ||
                    (kind == BrowserSurfaceFailureKind.PROVIDER && !providerId.isNullOrBlank()),
            ) { "Only provider failures may carry a provider identity" }
        }
    }
}

internal enum class BrowserSurfaceFailureKind(val wireName: String) {
    SURFACE_STATE("surface_state"),
    PROVIDER("provider"),
}

internal data class VisibleBrowserText(
    val text: String,
    val inspectedElements: Int,
    val includedValues: Int,
    val protectedElements: Int,
    val textLimitTruncated: Boolean,
)

internal data class BrowserCapturePresentation(
    val page: JsonObject,
    val artifact: JsonObject,
    val completionProof: JsonObject,
)

internal fun resolveVisibleBrowserSurface(
    observation: AccessibilityObservation,
    browserPackages: Set<String>,
    previous: BrowserSurfaceBinding?,
    preferredPackage: String? = null,
    launchedByPhoneControl: Boolean = false,
): BrowserSurfaceResolution {
    if (browserPackages.isEmpty()) {
        return BrowserSurfaceResolution.Failure(
            "browser_provider_unavailable",
            "No installed browser exposes standard Custom Tabs.",
            retryable = true,
            kind = BrowserSurfaceFailureKind.PROVIDER,
            observationGeneration = observation.generation,
            providerId = BROWSER_CUSTOM_TABS_PROVIDER,
            providerState = CapabilityState.NEEDS_USER_STEP,
            requiredUserStep = "choose_custom_tabs_browser",
        )
    }
    val candidates = observation.windows.filter { window ->
        window.contentAccessible && !window.controllerOwned &&
            window.packageName in browserPackages &&
            (preferredPackage == null || window.packageName == preferredPackage)
    }
    val strongest = listOf<(AccessibilityWindowSnapshot) -> Boolean>(
        { it.active && it.focused },
        AccessibilityWindowSnapshot::focused,
        AccessibilityWindowSnapshot::active,
    ).firstNotNullOfOrNull { predicate ->
        candidates.filter(predicate).takeIf { it.isNotEmpty() }
    }.orEmpty()
    if (strongest.size != 1) {
        return BrowserSurfaceResolution.Failure(
            code = if (strongest.isEmpty()) "browser_surface_not_visible" else "browser_surface_ambiguous",
            message = if (strongest.isEmpty()) {
                "No exact foreground browser surface is visible."
            } else {
                "More than one foreground browser surface matches the current display state."
            },
            retryable = true,
            kind = BrowserSurfaceFailureKind.SURFACE_STATE,
            observationGeneration = observation.generation,
        )
    }
    val window = strongest.single()
    val sameSurface = previous?.takeIf {
        it.packageName == window.packageName &&
            it.displayId == window.displayId &&
            it.windowId == window.id.toLong() &&
            it.bounds == window.bounds
    }
    val binding = BrowserSurfaceBinding(
        surfaceId = sameSurface?.surfaceId ?: UUID.randomUUID().toString(),
        observationGeneration = observation.generation,
        displayId = window.displayId,
        windowId = window.id.toLong(),
        packageName = requireNotNull(window.packageName),
        bounds = window.bounds,
        observationTimestampMs = observation.observedAtMs,
        windowTitle = window.title,
        credentialContextKind = when {
            sameSurface != null -> sameSurface.credentialContextKind
            launchedByPhoneControl -> "custom_tab_shared_browser_state"
            else -> "attached_existing_browser_tab"
        },
        launchedByPhoneControl = launchedByPhoneControl || sameSurface?.launchedByPhoneControl == true,
    )
    val safeObservation = observation.copy(
        elements = observation.elements.map(AccessibilityElement::withoutProtectedText),
    )
    val elements = safeObservation.elements.filter { element ->
        element.visible &&
            !element.controllerOwned &&
            element.packageName == binding.packageName &&
            element.target.windowId == binding.windowId
    }
    return BrowserSurfaceResolution.Success(
        BrowserSurfaceSnapshot(
            binding = binding,
            observation = safeObservation,
            elements = elements,
            observedUrl = uniqueVisibleHttpUrl(elements),
        ),
    )
}

private fun AccessibilityElement.withoutProtectedText(): AccessibilityElement =
    if (isProtected) {
        copy(label = null, value = null, hint = null, stateDescription = null)
    } else {
        this
    }

internal fun captureVisibleBrowserText(
    elements: List<AccessibilityElement>,
    maxChars: Int = MAX_VISIBLE_TEXT_CHARS,
): VisibleBrowserText {
    require(maxChars > 0)
    val values = LinkedHashSet<String>()
    elements.forEach { element ->
        if (element.isProtected) return@forEach
        listOf(element.label, element.value, element.hint, element.stateDescription)
            .mapNotNull { it?.replace(WHITESPACE, " ")?.trim()?.takeIf(String::isNotEmpty) }
            .forEach(values::add)
    }
    val text = StringBuilder()
    var included = 0
    var truncated = false
    for (value in values) {
        val separator = if (text.isEmpty()) 0 else 1
        if (text.length + separator + value.length > maxChars) {
            val remaining = maxChars - text.length - separator
            if (separator == 1 && text.length < maxChars) text.append('\n')
            if (remaining > 0) text.append(value.take(remaining))
            truncated = true
            break
        }
        if (separator == 1) text.append('\n')
        text.append(value)
        included += 1
    }
    return VisibleBrowserText(
        text = text.toString(),
        inspectedElements = elements.size,
        includedValues = included,
        protectedElements = elements.count(AccessibilityElement::isProtected),
        textLimitTruncated = truncated,
    )
}

internal fun buildBrowserCapturePresentation(
    title: String,
    observedUrl: String?,
    observationTruncated: Boolean,
    capture: VisibleBrowserText,
    artifactInfo: JsonObject,
    captureSha256: String,
    includePreview: Boolean,
): BrowserCapturePresentation {
    val page = buildJsonObject {
        put("title", title)
        put("title_source", "accessibility_window")
        put("title_is_document_verified", false)
        put("url", observedUrl.orEmpty())
        put("url_source", if (observedUrl == null) "unavailable" else "visible_accessibility_node")
        put("url_is_document_verified", false)
        put("char_count", capture.text.length)
        put("word_count", capture.text.split(WHITESPACE).count(String::isNotBlank))
        put("inspected_accessibility_elements", capture.inspectedElements)
        put("included_accessibility_values", capture.includedValues)
        put("protected_accessibility_elements", capture.protectedElements)
        put("capture_truncated", observationTruncated || capture.textLimitTruncated)
        put("capture_complete", false)
        put(
            "visible_accessibility_capture_complete",
            !observationTruncated && !capture.textLimitTruncated,
        )
        if (includePreview) put("text", capture.text.take(READ_PREVIEW_CHARS))
        if (includePreview) put("truncated", capture.text.length > READ_PREVIEW_CHARS)
    }
    val artifact = buildJsonObject {
        artifactInfo.forEach { (key, value) -> put(key, value) }
        put("capture_sha256", captureSha256)
        if (includePreview) put("preview", capture.text.take(ARTIFACT_PREVIEW_CHARS))
    }
    val completionProof = buildJsonObject {
        put("partial", buildJsonArray {
            if (includePreview) {
                add("/page/text")
                add("/artifact/preview")
            }
        })
        put("exact", buildJsonArray {
            add("/artifact/id")
            add("/artifact/sha256")
            add("/surface/surface_identity")
        })
    }
    return BrowserCapturePresentation(page, artifact, completionProof)
}

private fun uniqueVisibleHttpUrl(elements: List<AccessibilityElement>): String? {
    val candidates = elements.filterNot(AccessibilityElement::isProtected).flatMap { element ->
        listOf(element.value, element.label, element.hint)
    }.mapNotNull { raw ->
        val value = raw?.trim()?.takeIf(String::isNotEmpty) ?: return@mapNotNull null
        runCatching { URI(value) }.getOrNull()?.takeIf { uri ->
            uri.scheme?.lowercase() in setOf("http", "https") && !uri.host.isNullOrBlank()
        }?.normalize()?.toString()
    }.distinct()
    return candidates.singleOrNull()
}

private val WHITESPACE = Regex("\\s+")
private const val MAX_VISIBLE_TEXT_CHARS = 128_000
private const val READ_PREVIEW_CHARS = 24_000
private const val ARTIFACT_PREVIEW_CHARS = 1_000

internal const val BROWSER_ACCESSIBILITY_PROVIDER = "accessibility"
internal const val BROWSER_CUSTOM_TABS_PROVIDER = "custom_tabs_session"

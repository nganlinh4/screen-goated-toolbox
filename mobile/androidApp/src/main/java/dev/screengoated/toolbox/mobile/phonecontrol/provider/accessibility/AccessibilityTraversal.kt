package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.graphics.Rect
import android.hardware.display.DisplayManager
import android.os.Build
import android.os.SystemClock
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import android.view.Display
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlTargetIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService

internal data class AccessibilityTargetLease(
    val id: Int,
    val identity: PhoneControlTargetIdentity,
    val surfaceLease: AccessibilitySurfaceLease,
    val childPath: List<Int>,
    val fingerprint: AccessibilityNodeFingerprint,
    val accessibilityOverlay: Boolean,
    val authority: AccessibilityTargetAuthority,
)

internal data class AccessibilityNodeFingerprint(
    val packageName: String,
    val className: String?,
    val viewId: String?,
    val bounds: TargetBounds,
    val actions: Set<Int>,
    val semanticContentHash: Int,
    val isProtected: Boolean,
)

internal data class AccessibilityCapture(
    val observation: AccessibilityObservation,
    val leases: Map<Int, AccessibilityTargetLease>,
)

internal fun captureAccessibilitySurface(
    service: SgtAccessibilityService,
    generation: Long,
    maxElements: Int = DEFAULT_MAX_ELEMENTS,
): AccessibilityCapture {
    require(generation > 0) { "generation must be positive" }
    require(maxElements in 1..MAX_ELEMENTS_LIMIT) { "maxElements is out of range" }
    val observedAt = SystemClock.elapsedRealtime()
    val activeRoot = service.rootInActiveWindow
    val displayManager = service.getSystemService(DisplayManager::class.java)
    val displayExtents = displayManager?.displays.orEmpty().mapNotNull { display ->
        val metrics = service.createDisplayContext(display).resources.displayMetrics
        validPlatformTargetBounds(0, 0, metrics.widthPixels, metrics.heightPixels)?.let { bounds ->
            AccessibilityDisplayExtent(display.displayId, bounds)
        }
    }
    val activeWindow = activeRoot?.window
    val activeWindowDisplayId = activeRoot?.let { root ->
        activeWindow?.takeIf { window ->
            window.id == root.windowId && window.displayId >= 0 &&
                (window.isActive || window.isFocused)
        }?.displayId
    }
    val sourceWindows = appendMissingAccessibilityWindow(
        windows = accessibilityWindows(service),
        candidate = activeWindowDisplayId?.let { displayId ->
            AccessibilityWindowOnDisplay(displayId, requireNotNull(activeWindow))
        },
        windowId = AccessibilityWindowInfo::getId,
    )
    val capturedWindows = sourceWindows
        .sortedByDescending { window -> window.window.layer }
        .mapNotNull { entry -> entry.capture(activeRoot) }
    val activeRootDescriptor = activeRoot?.let { root ->
        Rect().also(root::getBoundsInScreen).toTargetBoundsOrNull()?.let { bounds ->
            val displayId = activeWindowDisplayId ?: resolveActiveRootDisplay(bounds, displayExtents)
            displayId?.let {
                ActiveAccessibilityRoot(
                    displayId = it,
                    id = root.windowId,
                    packageName = root.packageName?.toString(),
                    bounds = bounds,
                    root = root,
                )
            }
        }
    }
    val windowCandidates = supplementMissingActiveRoot(capturedWindows, activeRootDescriptor)
    val authorityPolicy = resolveAccessibilityTargetAuthorityPolicy(service)
    val windows = windowCandidates
        .map { window ->
            window.copy(
                targetAuthority = authorityPolicy.classifyWindow(window, windowCandidates),
            )
        }
        .sortedWith(
            compareByDescending<CapturedAccessibilityWindow<AccessibilityNodeInfo>> { it.active }
                .thenByDescending { it.focused }
                .thenByDescending { it.layer },
        )
    val defaultDisplay = displayManager?.getDisplay(Display.DEFAULT_DISPLAY)
    val defaultDisplayDensity = defaultDisplay?.let { display ->
        service.createDisplayContext(display).resources.displayMetrics.densityDpi
    } ?: service.resources.displayMetrics.densityDpi
    val windowSnapshots = snapshotAccessibilityWindows(windows, service.packageName)
    val elements = mutableListOf<AccessibilityElement>()
    val leases = linkedMapOf<Int, AccessibilityTargetLease>()
    val surfaceLeases = windowSnapshots.associate { window ->
        (window.displayId to window.id) to window.surfaceLease(generation)
    }
    var truncated = false

    for (window in windows) {
        val root = window.root ?: continue
        val surfaceLease = surfaceLeases[window.displayId to window.id] ?: continue
        traverseAccessibilityTree(
            node = root,
            path = emptyList(),
            windowId = window.id,
            displayId = window.displayId,
            overlay = window.accessibilityOverlay,
            controllerOwned = surfaceLease.controllerOwned,
            generation = generation,
            observedAt = observedAt,
            elements = elements,
            leases = leases,
            authorityPolicy = authorityPolicy,
            windowAuthority = window.targetAuthority,
            surfaceLease = surfaceLease,
            maxElements = maxElements,
            onTruncated = { truncated = true },
        )
    }

    return AccessibilityCapture(
        observation = AccessibilityObservation(
            generation = generation,
            observedAtMs = observedAt,
            displayRotation = defaultDisplay?.rotation ?: 0,
            densityDpi = defaultDisplayDensity,
            windows = windowSnapshots,
            elements = elements,
            truncated = truncated,
        ),
        leases = leases,
    )
}

internal fun accessibilityWindows(
    service: SgtAccessibilityService,
): List<AccessibilityWindowOnDisplay<AccessibilityWindowInfo>> = selectAccessibilityWindows(
    apiLevel = Build.VERSION.SDK_INT,
    defaultWindows = { service.windows },
    allDisplayWindows = {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val windowsByDisplay = service.windowsOnAllDisplays
            buildList {
                for (index in 0 until windowsByDisplay.size()) {
                    add(
                        AccessibilityDisplayWindows(
                            displayId = windowsByDisplay.keyAt(index),
                            windows = windowsByDisplay.valueAt(index).orEmpty(),
                        ),
                    )
                }
            }
        } else {
            emptyList()
        }
    },
)

internal fun findAccessibilityWindowRoot(
    service: SgtAccessibilityService,
    displayId: Int,
    windowId: Long,
): AccessibilityNodeInfo? {
    val listedRoot = accessibilityWindows(service).firstOrNull { entry ->
        entry.displayId == displayId && entry.window.id.toLong() == windowId
    }?.window?.root
    if (listedRoot != null) return listedRoot
    val activeRoot = service.rootInActiveWindow?.takeIf { root -> root.windowId.toLong() == windowId }
        ?: return null
    val rootBounds = Rect().also(activeRoot::getBoundsInScreen).toTargetBoundsOrNull() ?: return null
    val displayManager = service.getSystemService(DisplayManager::class.java) ?: return null
    val displayExtents = displayManager.displays.mapNotNull { display ->
        val metrics = service.createDisplayContext(display).resources.displayMetrics
        validPlatformTargetBounds(0, 0, metrics.widthPixels, metrics.heightPixels)?.let { bounds ->
            AccessibilityDisplayExtent(display.displayId, bounds)
        }
    }
    val activeDisplayId = activeRoot.window?.takeIf { window ->
        window.id == activeRoot.windowId && window.displayId >= 0 &&
            (window.isActive || window.isFocused)
    }?.displayId ?: resolveActiveRootDisplay(rootBounds, displayExtents)
    return activeRoot.takeIf { activeDisplayId == displayId }
}

private fun AccessibilityWindowOnDisplay<AccessibilityWindowInfo>.capture(
    activeRoot: AccessibilityNodeInfo?,
): CapturedAccessibilityWindow<AccessibilityNodeInfo>? {
    val listedRoot = window.root
    val root = selectAccessibilityWindowRoot(
        listedRoot = listedRoot,
        activeRoot = activeRoot,
        windowId = window.id,
        active = window.isActive,
        focused = window.isFocused,
        rootWindowId = { node -> node.windowId },
    )
    val windowRect = Rect().also(window::getBoundsInScreen)
    val rootRect = root?.let { node -> Rect().also(node::getBoundsInScreen) }
    val bounds = windowRect.toTargetBoundsOrNull()
        ?: rootRect?.toTargetBoundsOrNull()
        ?: run {
            Log.d(
                ACCESSIBILITY_CAPTURE_TAG,
                "window_dropped display=$displayId id=${window.id} type=${window.type} " +
                    "active=${window.isActive} focused=${window.isFocused} " +
                    "listed_root=${listedRoot != null} active_root=${activeRoot != null} " +
                    "window_bounds=${windowRect.flattenToString()} " +
                    "root_bounds=${rootRect?.flattenToString() ?: "none"}",
            )
            return null
        }
    return CapturedAccessibilityWindow(
        displayId = displayId,
        id = window.id,
        layer = window.layer,
        type = windowTypeName(window.type),
        title = window.title?.toString(),
        packageName = root?.packageName?.toString(),
        active = window.isActive,
        focused = window.isFocused,
        bounds = bounds,
        accessibilityOverlay = window.type == AccessibilityWindowInfo.TYPE_ACCESSIBILITY_OVERLAY,
        pictureInPicture = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            window.isInPictureInPictureMode
        } else {
            false
        },
        root = root,
    )
}

private fun traverseAccessibilityTree(
    node: AccessibilityNodeInfo,
    path: List<Int>,
    windowId: Int,
    displayId: Int,
    overlay: Boolean,
    controllerOwned: Boolean,
    generation: Long,
    observedAt: Long,
    elements: MutableList<AccessibilityElement>,
    leases: MutableMap<Int, AccessibilityTargetLease>,
    authorityPolicy: AccessibilityTargetAuthorityPolicy,
    windowAuthority: AccessibilityTargetAuthority,
    surfaceLease: AccessibilitySurfaceLease,
    maxElements: Int,
    onTruncated: () -> Unit,
) {
    if (elements.size >= maxElements) {
        onTruncated()
        return
    }
    val packageName = node.packageName?.toString().orEmpty().ifBlank { "unknown" }
    val targetAuthority = strongestAccessibilityAuthority(
        strongestAccessibilityAuthority(
            windowAuthority,
            authorityPolicy.classify(packageName),
        ),
        structuralNodeAuthority(
            node.supportsAction(AccessibilityNodeInfo.ACTION_DISMISS),
        ),
    )
    val bounds = Rect().also(node::getBoundsInScreen).toTargetBoundsOrNull()
    val actions = node.actionList.mapNotNull { actionName(it.id) }.toSet()
    val editable = node.isEditable || node.supportsAction(AccessibilityNodeInfo.ACTION_SET_TEXT)
    val content = node.accessibilityContent(editable)
    val meaningful = node.isVisibleToUser && (
        content.label != null || content.hint != null || content.stateDescription != null ||
            actions.isNotEmpty() ||
            node.isCheckable || node.isEditable || node.isScrollable
        )
    if (meaningful && bounds != null && bounds.right > bounds.left && bounds.bottom > bounds.top) {
        val id = elements.size + 1
        val nodeIdentity = buildString {
            append(windowId).append(':')
            append(path.joinToString("."))
            node.viewIdResourceName?.let { append(':').append(it) }
        }
        val identity = PhoneControlTargetIdentity(
            snapshotGeneration = generation,
            displayId = displayId,
            windowId = windowId.toLong(),
            packageOrSurface = packageName,
            nodeOrDocumentIdentity = nodeIdentity,
            bounds = bounds,
            observationTimestampMs = observedAt,
        )
        val fingerprint = AccessibilityNodeFingerprint(
            packageName = packageName,
            className = node.className?.toString(),
            viewId = node.viewIdResourceName,
            bounds = bounds,
            actions = node.actionList.map { it.id }.toSet(),
            semanticContentHash = content.semanticFingerprintHash,
            isProtected = content.isProtected,
        )
        elements += AccessibilityElement(
            id = id,
            role = semanticRole(node),
            label = content.label,
            value = content.value,
            hint = content.hint,
            stateDescription = content.stateDescription,
            viewId = node.viewIdResourceName,
            packageName = packageName,
            className = node.className?.toString(),
            bounds = bounds,
            actions = actions,
            enabled = node.isEnabled,
            visible = node.isVisibleToUser,
            focused = node.isFocused,
            selected = node.isSelected,
            checked = node.checkedBoolean().takeIf { node.isCheckable },
            isProtected = content.isProtected,
            controllerOwned = controllerOwned,
            target = identity,
            targetAuthority = targetAuthority,
        )
        leases[id] = AccessibilityTargetLease(
            id = id,
            identity = identity,
            surfaceLease = surfaceLease,
            childPath = path,
            fingerprint = fingerprint,
            accessibilityOverlay = overlay,
            authority = targetAuthority,
        )
    }

    for (index in 0 until node.childCount) {
        if (elements.size >= maxElements) {
            onTruncated()
            break
        }
        node.getChild(index)?.let { child ->
            traverseAccessibilityTree(
                node = child,
                path = path + index,
                windowId = windowId,
                displayId = displayId,
                overlay = overlay,
                controllerOwned = controllerOwned,
                generation = generation,
                observedAt = observedAt,
                elements = elements,
                leases = leases,
                authorityPolicy = authorityPolicy,
                windowAuthority = windowAuthority,
                surfaceLease = surfaceLease,
                maxElements = maxElements,
                onTruncated = onTruncated,
            )
        }
    }
}

internal fun AccessibilityNodeInfo.matches(lease: AccessibilityTargetLease): Boolean {
    return currentFingerprint() == lease.fingerprint
}

internal fun AccessibilityNodeInfo.matchesIgnoringActions(lease: AccessibilityTargetLease): Boolean {
    val current = currentFingerprint() ?: return false
    return current.copy(actions = lease.fingerprint.actions) == lease.fingerprint
}

private fun AccessibilityNodeInfo.currentFingerprint(): AccessibilityNodeFingerprint? {
    val bounds = Rect().also(::getBoundsInScreen).toTargetBoundsOrNull() ?: return null
    val editable = isEditable || supportsAction(AccessibilityNodeInfo.ACTION_SET_TEXT)
    val content = accessibilityContent(editable)
    return AccessibilityNodeFingerprint(
        packageName = packageName?.toString().orEmpty().ifBlank { "unknown" },
        className = className?.toString(),
        viewId = viewIdResourceName,
        bounds = bounds,
        actions = actionList.map { it.id }.toSet(),
        semanticContentHash = content.semanticFingerprintHash,
        isProtected = content.isProtected,
    )
}

private fun AccessibilityNodeInfo.accessibilityContent(editable: Boolean): AccessibilityNodeContent =
    accessibilityNodeContent(
        isPassword = isPassword,
        contentDescription = contentDescription?.toString(),
        text = text?.toString(),
        hint = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) hintText?.toString() else null,
        stateDescription = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            stateDescription?.toString()
        } else {
            null
        },
        editable = editable,
    )

private fun semanticRole(node: AccessibilityNodeInfo): String {
    val className = node.className?.toString().orEmpty().substringAfterLast('.')
    return when {
        node.isEditable -> "text_field"
        node.isCheckable && className.contains("Switch", ignoreCase = true) -> "switch"
        node.isCheckable -> "checkbox"
        node.isScrollable -> "scroll_container"
        className.contains("Button", ignoreCase = true) -> "button"
        className.contains("Image", ignoreCase = true) -> "image"
        className.contains("WebView", ignoreCase = true) -> "web_content"
        className.contains("Recycler", ignoreCase = true) ||
            className.contains("List", ignoreCase = true) -> "list"
        else -> className.ifBlank { "node" }.lowercase()
    }
}

private fun actionName(id: Int): String? = when (id) {
    AccessibilityNodeInfo.ACTION_CLICK -> "click"
    AccessibilityNodeInfo.ACTION_LONG_CLICK -> "long_click"
    AccessibilityNodeInfo.ACTION_FOCUS -> "focus"
    AccessibilityNodeInfo.ACTION_SET_TEXT -> "fill"
    AccessibilityNodeInfo.ACTION_SELECT -> "select"
    AccessibilityNodeInfo.ACTION_SCROLL_FORWARD -> "scroll_forward"
    AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD -> "scroll_backward"
    AccessibilityNodeInfo.ACTION_EXPAND -> "expand"
    AccessibilityNodeInfo.ACTION_COLLAPSE -> "collapse"
    AccessibilityNodeInfo.ACTION_DISMISS -> "dismiss"
    else -> null
}

private fun windowTypeName(type: Int): String = when (type) {
    AccessibilityWindowInfo.TYPE_APPLICATION -> "application"
    AccessibilityWindowInfo.TYPE_INPUT_METHOD -> "input_method"
    AccessibilityWindowInfo.TYPE_SYSTEM -> "system"
    AccessibilityWindowInfo.TYPE_ACCESSIBILITY_OVERLAY -> "accessibility_overlay"
    AccessibilityWindowInfo.TYPE_SPLIT_SCREEN_DIVIDER -> "split_screen_divider"
    else -> "unknown"
}

private fun Rect.toTargetBoundsOrNull(): TargetBounds? = validPlatformTargetBounds(
    left = left,
    top = top,
    right = right,
    bottom = bottom,
)

internal fun validPlatformTargetBounds(
    left: Int,
    top: Int,
    right: Int,
    bottom: Int,
): TargetBounds? = if (right < left || bottom < top) {
    null
} else {
    TargetBounds(left, top, right, bottom)
}

internal fun AccessibilityNodeInfo.supportsAction(actionId: Int): Boolean =
    actionList.any { it.id == actionId }

@Suppress("DEPRECATION")
internal fun AccessibilityNodeInfo.checkedBoolean(): Boolean =
    if (Build.VERSION.SDK_INT >= 36) {
        checked == AccessibilityNodeInfo.CHECKED_STATE_TRUE
    } else {
        isChecked
    }

private const val DEFAULT_MAX_ELEMENTS = 400
private const val MAX_ELEMENTS_LIMIT = 1_000
private const val ACCESSIBILITY_CAPTURE_TAG = "SGTPhoneControlCapture"

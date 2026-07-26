package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.os.SystemClock
import android.view.accessibility.AccessibilityEvent
import java.util.concurrent.atomic.AtomicLong
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log

internal enum class AccessibilityInvalidationImpact {
    NONE,
    SEMANTIC_ONLY,
    HARD,
}

internal fun accessibilityInvalidationImpact(
    eventType: Int,
    contentChangeTypes: Int,
    windowChanges: Int = 0,
): AccessibilityInvalidationImpact = when (eventType) {
    AccessibilityEvent.TYPE_WINDOWS_CHANGED -> windowsChangedImpact(windowChanges)

    AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED -> AccessibilityInvalidationImpact.HARD

    AccessibilityEvent.TYPE_VIEW_CLICKED,
    AccessibilityEvent.TYPE_VIEW_SCROLLED,
    AccessibilityEvent.TYPE_VIEW_TEXT_CHANGED,
    AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED ->
        if (contentChangeTypes >= 0) {
            AccessibilityInvalidationImpact.SEMANTIC_ONLY
        } else {
            AccessibilityInvalidationImpact.HARD
        }

    AccessibilityEvent.TYPE_VIEW_FOCUSED,
    AccessibilityEvent.TYPE_VIEW_SELECTED,
    AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED,
    -> AccessibilityInvalidationImpact.SEMANTIC_ONLY

    else -> AccessibilityInvalidationImpact.NONE
}

private fun windowsChangedImpact(windowChanges: Int): AccessibilityInvalidationImpact {
    if (windowChanges == 0) return AccessibilityInvalidationImpact.HARD
    return if (windowChanges and HARD_WINDOW_CHANGE_MASK != 0) {
        AccessibilityInvalidationImpact.HARD
    } else {
        AccessibilityInvalidationImpact.SEMANTIC_ONLY
    }
}

internal object AccessibilityInvalidationDiagnostics {
    private val hardCount = AtomicLong()
    private val semanticCount = AtomicLong()
    private val lastLogMs = AtomicLong()

    fun record(
        impact: AccessibilityInvalidationImpact,
        eventType: Int,
        contentChangeTypes: Int,
        windowChanges: Int,
        windowId: Int,
        sourcePackage: String,
        generation: Long,
        visualRevision: Long,
    ) {
        when (impact) {
            AccessibilityInvalidationImpact.HARD -> hardCount.incrementAndGet()
            AccessibilityInvalidationImpact.SEMANTIC_ONLY -> {
                semanticCount.incrementAndGet()
                return
            }
            AccessibilityInvalidationImpact.NONE -> return
        }
        val now = SystemClock.elapsedRealtime()
        val previous = lastLogMs.get()
        if (now - previous < LOG_INTERVAL_MS || !lastLogMs.compareAndSet(previous, now)) return
        Log.d(
            TAG,
            "invalidation_hard hard=${hardCount.getAndSet(0)} " +
                "semantic_since_hard=${semanticCount.getAndSet(0)} event_type=$eventType " +
                "content_changes=$contentChangeTypes window_changes=$windowChanges " +
                "window_id=$windowId source=$sourcePackage " +
                "generation=$generation visual_revision=$visualRevision",
        )
    }

    private const val LOG_INTERVAL_MS = 1_000L
    private const val TAG = "SGTPhoneControlAccessibility"
}

private const val HARD_WINDOW_CHANGE_MASK =
    AccessibilityEvent.WINDOWS_CHANGE_ADDED or
        AccessibilityEvent.WINDOWS_CHANGE_REMOVED or
        AccessibilityEvent.WINDOWS_CHANGE_BOUNDS or
        AccessibilityEvent.WINDOWS_CHANGE_LAYER or
        AccessibilityEvent.WINDOWS_CHANGE_ACTIVE or
        AccessibilityEvent.WINDOWS_CHANGE_FOCUSED or
        AccessibilityEvent.WINDOWS_CHANGE_PARENT or
        AccessibilityEvent.WINDOWS_CHANGE_CHILDREN or
        AccessibilityEvent.WINDOWS_CHANGE_PIP

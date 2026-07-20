package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import android.os.SystemClock
import android.view.accessibility.AccessibilityEvent
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log

internal enum class AccessibilityInvalidationImpact {
    NONE,
    SEMANTIC_ONLY,
    HARD,
}

internal fun accessibilityInvalidationImpact(
    eventType: Int,
    contentChangeTypes: Int,
): AccessibilityInvalidationImpact = when (eventType) {
    AccessibilityEvent.TYPE_WINDOWS_CHANGED,
    AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED,
    AccessibilityEvent.TYPE_VIEW_CLICKED,
    AccessibilityEvent.TYPE_VIEW_SCROLLED,
    AccessibilityEvent.TYPE_VIEW_TEXT_CHANGED,
    -> AccessibilityInvalidationImpact.HARD

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

internal object AccessibilityInvalidationDiagnostics {
    private val hardCount = AtomicLong()
    private val semanticCount = AtomicLong()
    private val lastLogMs = AtomicLong()
    private val lastSignature = AtomicReference("none")

    fun record(
        impact: AccessibilityInvalidationImpact,
        eventType: Int,
        contentChangeTypes: Int,
        windowId: Int,
        sourcePackage: String,
        generation: Long,
        visualRevision: Long,
    ) {
        when (impact) {
            AccessibilityInvalidationImpact.HARD -> hardCount.incrementAndGet()
            AccessibilityInvalidationImpact.SEMANTIC_ONLY -> semanticCount.incrementAndGet()
            AccessibilityInvalidationImpact.NONE -> return
        }
        lastSignature.set(
            "event=$eventType content_changes=$contentChangeTypes " +
                "window=$windowId source=$sourcePackage",
        )
        val now = SystemClock.elapsedRealtime()
        val previous = lastLogMs.get()
        if (now - previous < LOG_INTERVAL_MS || !lastLogMs.compareAndSet(previous, now)) return
        Log.d(
            TAG,
            "invalidation_summary hard=${hardCount.getAndSet(0)} " +
                "semantic_only=${semanticCount.getAndSet(0)} last=${lastSignature.get()} " +
                "generation=$generation visual_revision=$visualRevision",
        )
    }

    private const val LOG_INTERVAL_MS = 5_000L
    private const val TAG = "SGTPhoneControlAccessibility"
}

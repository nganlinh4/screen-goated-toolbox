package dev.screengoated.toolbox.mobile.service

import android.accessibilityservice.AccessibilityServiceInfo
import android.annotation.SuppressLint
import android.view.accessibility.AccessibilityEvent

internal data class AccessibilityEventProfile(
    val eventTypes: Int,
    val notificationTimeoutMs: Long,
    val flags: Int,
)

private const val BASE_FLAGS = AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS or
    AccessibilityServiceInfo.FLAG_INCLUDE_NOT_IMPORTANT_VIEWS or
    AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS

private const val API_TIRAMISU = 33

/**
 * Text selection needs only selection and focus events. Phone control needs
 * every event at a short timeout to keep its observations current, so the
 * service widens to that only while a phone-control service is alive; an idle
 * enabled service no longer wakes SGT for every scroll or content change.
 */
@SuppressLint("InlinedApi") // FLAG_INPUT_METHOD_EDITOR is applied only on API 33+.
internal fun accessibilityEventProfile(phoneControlActive: Boolean, sdkInt: Int): AccessibilityEventProfile =
    if (phoneControlActive) {
        AccessibilityEventProfile(
            eventTypes = AccessibilityEvent.TYPES_ALL_MASK,
            notificationTimeoutMs = 50L,
            flags = if (sdkInt >= API_TIRAMISU) BASE_FLAGS or AccessibilityServiceInfo.FLAG_INPUT_METHOD_EDITOR else BASE_FLAGS,
        )
    } else {
        AccessibilityEventProfile(
            eventTypes = AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED or AccessibilityEvent.TYPE_VIEW_FOCUSED,
            notificationTimeoutMs = 100L,
            flags = BASE_FLAGS,
        )
    }

/**
 * Tracks which owners need the full accessibility event stream and applies the
 * matching profile to the connected service. Owners are idempotent: releasing
 * twice, or acquiring twice, never unbalances the demand.
 */
internal class AccessibilityEventDemand {
    private val owners = HashSet<Any>()
    private var target: Any? = null
    private var applier: ((phoneControlActive: Boolean) -> Unit)? = null
    private var applied: Boolean? = null

    @Synchronized
    fun acquire(owner: Any) {
        owners += owner
        applyLocked()
    }

    @Synchronized
    fun release(owner: Any) {
        owners -= owner
        applyLocked()
    }

    /** A newly connected service always receives the current profile. */
    @Synchronized
    fun attach(service: Any, applyProfile: (phoneControlActive: Boolean) -> Unit) {
        target = service
        applier = applyProfile
        applied = null
        applyLocked()
    }

    @Synchronized
    fun detach(service: Any) {
        if (target !== service) return
        target = null
        applier = null
        applied = null
    }

    private fun applyLocked() {
        val active = owners.isNotEmpty()
        if (applied == active) return
        val applyProfile = applier ?: return
        applyProfile(active)
        applied = active
    }
}

internal val phoneControlAccessibilityDemand = AccessibilityEventDemand()

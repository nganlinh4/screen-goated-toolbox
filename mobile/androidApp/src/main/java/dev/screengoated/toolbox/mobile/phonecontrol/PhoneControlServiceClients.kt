package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayController
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayExclusion
import dev.screengoated.toolbox.mobile.service.phoneControlAccessibilityDemand

/**
 * Shared clients a live phone-control service owns: input paths avoid its
 * overlay, and the accessibility service streams every event only while it lives.
 */
internal fun registerPhoneControlServiceClients(overlay: PhoneControlOverlayController) {
    PhoneControlOverlayExclusion.register(overlay)
    phoneControlAccessibilityDemand.acquire(overlay)
}

internal fun unregisterPhoneControlServiceClients(overlay: PhoneControlOverlayController) {
    phoneControlAccessibilityDemand.release(overlay)
    PhoneControlOverlayExclusion.unregister(overlay)
}

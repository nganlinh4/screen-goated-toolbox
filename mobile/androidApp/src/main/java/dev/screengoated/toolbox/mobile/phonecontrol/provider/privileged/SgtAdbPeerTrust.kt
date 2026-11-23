package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

/** Availability depends on authenticating the actual command connection's peer. */
internal object SgtAdbPeerTrust {
    // The current transport does not expose a verified binding between pairing
    // and the separate command endpoint. Discovery names are not credentials.
    fun isAvailable(): Boolean = false
}

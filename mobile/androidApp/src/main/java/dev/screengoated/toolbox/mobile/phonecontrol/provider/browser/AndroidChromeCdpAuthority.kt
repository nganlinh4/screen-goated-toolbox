package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.BrowserTunnelResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbCommandBridge
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuCommandBridge
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerChoice
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences

internal data class ChromeCdpAuthorityProbe(
    val state: CapabilityState,
    val providerId: String? = null,
    val requiredUserStep: String? = null,
)

internal sealed interface ChromeCdpTunnelOpenResult {
    data class Ready(
        val providerId: String,
        val lease: BrowserTunnelResult.Ready,
    ) : ChromeCdpTunnelOpenResult

    data class Failure(val result: BrowserTunnelResult.Failure) : ChromeCdpTunnelOpenResult
}

/**
 * Single structural owner for Chrome-stream authority selection. Command
 * authority and browser-stream authority are separate capabilities: a provider
 * is advertised for CDP only when it exposes a proven duplex socket route.
 */
internal object AndroidChromeCdpAuthority {
    fun probe(context: Context): ChromeCdpAuthorityProbe =
        when (PhoneControlPowerPreferences.current(context)) {
            PhoneControlPowerChoice.SGT_ADB -> SgtAdbCommandBridge.probe(context).let {
                ChromeCdpAuthorityProbe(it.state, SGT_PROVIDER, it.requiredUserStep)
            }
            PhoneControlPowerChoice.SHIZUKU -> ShizukuCommandBridge.probe(context).let {
                ChromeCdpAuthorityProbe(it.state, SHIZUKU_PROVIDER, it.requiredUserStep)
            }
            PhoneControlPowerChoice.ROOT -> ChromeCdpAuthorityProbe(
                state = CapabilityState.UNSUPPORTED,
                providerId = ROOT_PROVIDER,
                requiredUserStep = "Select SGT Bridge or Shizuku for Chrome DevTools control.",
            )
            PhoneControlPowerChoice.STANDARD, null -> ChromeCdpAuthorityProbe(
                state = CapabilityState.UNAVAILABLE,
                requiredUserStep = "Select SGT Bridge or Shizuku for Chrome DevTools control.",
            )
        }

    suspend fun open(context: Context): ChromeCdpTunnelOpenResult {
        val probe = probe(context)
        val providerId = probe.providerId
            ?: return ChromeCdpTunnelOpenResult.Failure(
                BrowserTunnelResult.Failure(
                    probe.state,
                    "browser_authority_unavailable",
                    probe.requiredUserStep,
                ),
            )
        val opened = when (providerId) {
            SGT_PROVIDER -> SgtAdbCommandBridge.openBrowserTunnel(context)
            SHIZUKU_PROVIDER -> ShizukuCommandBridge.openBrowserTunnel(context)
            else -> BrowserTunnelResult.Failure(
                probe.state,
                "browser_stream_unsupported",
                probe.requiredUserStep,
            )
        }
        return when (opened) {
            is BrowserTunnelResult.Ready ->
                ChromeCdpTunnelOpenResult.Ready(providerId, opened)
            is BrowserTunnelResult.Failure ->
                ChromeCdpTunnelOpenResult.Failure(opened)
        }
    }

    suspend fun close(context: Context, providerId: String, leaseId: String): Boolean =
        when (providerId) {
            SGT_PROVIDER -> SgtAdbCommandBridge.closeBrowserTunnel(context, leaseId)
            SHIZUKU_PROVIDER -> ShizukuCommandBridge.closeBrowserTunnel(context, leaseId)
            else -> false
        }

    private const val SGT_PROVIDER = "sgt_adb_bridge"
    private const val SHIZUKU_PROVIDER = "shizuku_shell"
    private const val ROOT_PROVIDER = "root_bridge"
}

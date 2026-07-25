package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import io.github.muntashirakon.adb.AdbStream
import java.io.Closeable

/**
 * Provider shim from the first-party ADB stream to the shared authenticated
 * browser tunnel. The bridge process, not the app process, owns both ends.
 */
internal class SgtAdbBrowserTunnel(
    manager: SgtAdbConnectionManager,
) : Closeable {
    private val delegate = AuthenticatedBrowserTunnel {
        AdbBrowserTunnelUpstream(manager.openStream(CHROME_DEVTOOLS_DESTINATION))
    }
    val lease: BrowserTunnelLease
        get() = delegate.lease

    val isOpen: Boolean
        get() = delegate.isOpen

    override fun close() = delegate.close()

    private class AdbBrowserTunnelUpstream(
        private val stream: AdbStream,
    ) : BrowserTunnelUpstream {
        override val input = stream.openInputStream()
        override val output = stream.openOutputStream()
        override fun close() = stream.close()
    }

    private companion object {
        const val CHROME_DEVTOOLS_DESTINATION = "localabstract:chrome_devtools_remote"
    }
}

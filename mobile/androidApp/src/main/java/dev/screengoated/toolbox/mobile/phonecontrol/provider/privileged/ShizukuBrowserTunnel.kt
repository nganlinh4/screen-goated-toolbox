package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.net.LocalSocket
import android.net.LocalSocketAddress
import java.io.Closeable

/**
 * Chrome DevTools tunnel owned by the Shizuku user-service process. A fresh
 * abstract-socket connection is created for each authenticated loopback client.
 */
internal class ShizukuBrowserTunnel : Closeable {
    private val delegate = AuthenticatedBrowserTunnel {
        val socket = LocalSocket()
        try {
            socket.connect(
                LocalSocketAddress(
                    CHROME_DEVTOOLS_SOCKET,
                    LocalSocketAddress.Namespace.ABSTRACT,
                ),
            )
            LocalBrowserTunnelUpstream(socket)
        } catch (error: Throwable) {
            runCatching(socket::close)
            throw error
        }
    }
    val lease: BrowserTunnelLease
        get() = delegate.lease

    val isOpen: Boolean
        get() = delegate.isOpen

    override fun close() = delegate.close()

    private class LocalBrowserTunnelUpstream(
        private val socket: LocalSocket,
    ) : BrowserTunnelUpstream {
        override val input = socket.inputStream
        override val output = socket.outputStream
        override fun close() = socket.close()
    }

    private companion object {
        const val CHROME_DEVTOOLS_SOCKET = "chrome_devtools_remote"
    }
}

package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress

internal fun publicNetworkHost(host: String): Boolean {
    val normalized = host.trim().trimEnd('.')
    if (
        normalized.isBlank() ||
        normalized.equals("localhost", ignoreCase = true) ||
        normalized.endsWith(".localhost", ignoreCase = true)
    ) {
        return false
    }
    if (!looksLikeAddressLiteral(normalized)) return true
    val address = runCatching { InetAddress.getByName(normalized) }.getOrNull() ?: return false
    return publicNetworkAddress(address)
}

internal fun publicNetworkAddress(address: InetAddress): Boolean {
    if (
        address.isAnyLocalAddress ||
        address.isLoopbackAddress ||
        address.isLinkLocalAddress ||
        address.isSiteLocalAddress ||
        address.isMulticastAddress
    ) {
        return false
    }
    val bytes = address.address
    return when (address) {
        is Inet4Address -> {
            val first = bytes[0].toInt() and 0xff
            val second = bytes[1].toInt() and 0xff
            !(first == 0 ||
                (first == 100 && second in 64..127) ||
                (first == 192 && second == 0) ||
                (first == 198 && second in 18..19) ||
                first >= 224)
        }
        is Inet6Address -> (bytes[0].toInt() and 0xfe) != 0xfc
        else -> false
    }
}

private fun looksLikeAddressLiteral(host: String): Boolean =
    ':' in host || host.all { it.isDigit() || it == '.' }

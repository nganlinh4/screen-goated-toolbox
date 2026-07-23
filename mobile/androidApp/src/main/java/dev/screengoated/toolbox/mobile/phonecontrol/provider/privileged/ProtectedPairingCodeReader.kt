package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.Context
import android.content.Intent
import android.os.SystemClock
import android.provider.Settings
import android.view.accessibility.AccessibilityNodeInfo
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import kotlinx.coroutines.delay

internal object ProtectedPairingCodeReader {
    suspend fun await(
        context: Context,
        token: PhoneControlProtectedCheckpointToken,
        timeoutMs: Long = DEFAULT_TIMEOUT_MS,
    ): CharArray? {
        val service = SgtAccessibilityService.instance ?: return null
        val settingsPackage = Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS)
            .resolveActivity(context.packageManager)
            ?.packageName
            ?: return null
        val deadline = SystemClock.elapsedRealtime() + timeoutMs
        do {
            if (!PhoneControlProtectedCheckpointRegistry.owns(token)) return null
            val candidates = accessibilityRoots(service)
                .filter { it.packageName?.toString() == settingsPackage }
                .mapNotNull { root ->
                    protectedPairingCode(
                        accessibilityNodes(root).map { node ->
                            ProtectedPairingSurfaceValue(
                                text = node.text,
                                resourceId = node.viewIdResourceName,
                                editable = node.isEditable,
                            )
                        },
                    )
                }
            val result = uniqueAsciiOneTimeCode(candidates.map(CharArray::concatToString))
            candidates.forEach { it.fill('\u0000') }
            result?.let { return it }
            delay(POLL_INTERVAL_MS)
        } while (SystemClock.elapsedRealtime() < deadline)
        return null
    }
}

internal fun accessibilityRoots(
    service: SgtAccessibilityService,
): List<AccessibilityNodeInfo> = service.windows.mapNotNull { it.root }

internal fun accessibilityNodes(root: AccessibilityNodeInfo): List<AccessibilityNodeInfo> {
    val nodes = ArrayList<AccessibilityNodeInfo>()
    val pending = ArrayDeque<AccessibilityNodeInfo>()
    pending.add(root)
    while (pending.isNotEmpty()) {
        val node = pending.removeFirst()
        nodes += node
        repeat(node.childCount) { index ->
            node.getChild(index)?.let(pending::addLast)
        }
    }
    return nodes
}

internal fun uniqueAsciiOneTimeCode(values: List<CharSequence?>): CharArray? {
    val candidates = linkedSetOf<String>()
    values.forEach { value ->
        val text = value ?: return@forEach
        if (text.length != ONE_TIME_CODE_LENGTH || text.any { it !in '0'..'9' }) {
            return@forEach
        }
        candidates += text.toString()
    }
    return candidates.singleOrNull()?.toCharArray()
}

internal data class ProtectedPairingSurfaceValue(
    val text: CharSequence?,
    val resourceId: String?,
    val editable: Boolean,
)

internal fun protectedPairingCode(
    values: List<ProtectedPairingSurfaceValue>,
): CharArray? {
    val code = uniqueAsciiOneTimeCode(
        values.filterNot(ProtectedPairingSurfaceValue::editable).map { it.text },
    ) ?: return null
    val resourceEntries = values.mapNotNull { it.resourceId?.substringAfterLast('/') }
    val canonicalStructure =
        PAIRING_CODE_RESOURCE in resourceEntries &&
            PAIRING_CONTAINER_RESOURCE in resourceEntries &&
            PAIRING_ADDRESS_RESOURCE in resourceEntries
    val genericStructure =
        values.none(ProtectedPairingSurfaceValue::editable) &&
            values.any { isPairingEndpoint(it.text?.toString().orEmpty()) }
    if (canonicalStructure || genericStructure) return code
    code.fill('\u0000')
    return null
}

private fun isPairingEndpoint(value: String): Boolean {
    val separator = value.lastIndexOf(':')
    if (separator <= 0) return false
    val port = value.substring(separator + 1).toIntOrNull()
        ?.takeIf { it in 1..65_535 }
        ?: return false
    val rawHost = value.substring(0, separator)
    val host = rawHost.removePrefix("[").removeSuffix("]")
    if (host != rawHost && !(rawHost.startsWith("[") && rawHost.endsWith("]"))) return false
    if (':' in host) {
        return host.length <= MAX_IPV6_LITERAL_CHARS &&
            host.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' || it == ':' || it == '.' }
    }
    val octets = host.split('.')
    return port > 0 && octets.size == IPV4_OCTETS &&
        octets.all { octet ->
            octet.isNotEmpty() &&
                octet.all(Char::isDigit) &&
                octet.toIntOrNull()?.let { it in 0..255 } == true
        }
}

private const val ONE_TIME_CODE_LENGTH = 6
private const val POLL_INTERVAL_MS = 200L
private const val DEFAULT_TIMEOUT_MS = 5_000L
private const val PAIRING_CODE_RESOURCE = "pairing_code"
private const val PAIRING_CONTAINER_RESOURCE = "l_pairing_six_digit"
private const val PAIRING_ADDRESS_RESOURCE = "ip_addr"
private const val IPV4_OCTETS = 4
private const val MAX_IPV6_LITERAL_CHARS = 45

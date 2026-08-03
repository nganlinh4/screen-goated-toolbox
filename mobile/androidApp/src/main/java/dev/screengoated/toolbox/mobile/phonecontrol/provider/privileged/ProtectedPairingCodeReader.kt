package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.Context
import android.content.Intent
import android.os.SystemClock
import android.provider.Settings
import android.view.accessibility.AccessibilityNodeInfo
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointReadiness
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import kotlinx.coroutines.delay

internal object ProtectedPairingCodeReader {
    fun surfaceReadiness(context: Context): PhoneControlProtectedCheckpointReadiness {
        val service = SgtAccessibilityService.instance
            ?: return ProtectedPairingCodeFailure.ACCESSIBILITY_UNAVAILABLE.notReady()
        val settingsPackage = settingsPackage(context)
            ?: return ProtectedPairingCodeFailure.SETTINGS_HANDLER_UNAVAILABLE.notReady()
        val roots = accessibilityRoots(service)
            .filter { it.packageName?.toString() == settingsPackage }
        if (roots.isEmpty()) {
            return ProtectedPairingCodeFailure.SETTINGS_SURFACE_UNAVAILABLE.notReady()
        }
        return if (roots.any { protectedPairingSurfaceReady(accessibilitySurfaceValues(it)) }) {
            PhoneControlProtectedCheckpointReadiness.Ready
        } else {
            ProtectedPairingCodeFailure.PAIRING_SURFACE_NOT_READY.notReady()
        }
    }

    suspend fun await(
        context: Context,
        token: PhoneControlProtectedCheckpointToken,
        timeoutMs: Long = DEFAULT_TIMEOUT_MS,
    ): ProtectedPairingCodeReadResult {
        val service = SgtAccessibilityService.instance
            ?: return ProtectedPairingCodeReadResult.Unavailable(
                ProtectedPairingCodeFailure.ACCESSIBILITY_UNAVAILABLE,
            )
        val settingsPackage = settingsPackage(context)
            ?: return ProtectedPairingCodeReadResult.Unavailable(
                ProtectedPairingCodeFailure.SETTINGS_HANDLER_UNAVAILABLE,
            )
        val deadline = SystemClock.elapsedRealtime() + timeoutMs
        var settingsSurfaceSeen = false
        var pairingSurfaceSeen = false
        do {
            if (!PhoneControlProtectedCheckpointRegistry.owns(token)) {
                return ProtectedPairingCodeReadResult.Unavailable(
                    ProtectedPairingCodeFailure.CHECKPOINT_OWNER_LOST,
                )
            }
            val roots = accessibilityRoots(service)
                .filter { it.packageName?.toString() == settingsPackage }
            settingsSurfaceSeen = settingsSurfaceSeen || roots.isNotEmpty()
            val surfaces = roots.map(::accessibilitySurfaceValues)
            pairingSurfaceSeen = pairingSurfaceSeen || surfaces.any(::protectedPairingSurfaceReady)
            val candidates = surfaces.mapNotNull(::protectedPairingCode)
            val result = uniqueAsciiOneTimeCode(candidates.map(CharArray::concatToString))
            candidates.forEach { it.fill('\u0000') }
            result?.let { return ProtectedPairingCodeReadResult.Available(it) }
            delay(POLL_INTERVAL_MS)
        } while (SystemClock.elapsedRealtime() < deadline)
        val failure = when {
            !settingsSurfaceSeen -> ProtectedPairingCodeFailure.SETTINGS_SURFACE_UNAVAILABLE
            !pairingSurfaceSeen -> ProtectedPairingCodeFailure.PAIRING_SURFACE_NOT_READY
            else -> ProtectedPairingCodeFailure.PAIRING_CODE_UNAVAILABLE
        }
        return ProtectedPairingCodeReadResult.Unavailable(failure)
    }

    private fun settingsPackage(context: Context): String? =
        Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS)
            .resolveActivity(context.packageManager)
            ?.packageName
}

internal sealed interface ProtectedPairingCodeReadResult {
    data class Available(val code: CharArray) : ProtectedPairingCodeReadResult

    data class Unavailable(
        val failure: ProtectedPairingCodeFailure,
    ) : ProtectedPairingCodeReadResult
}

internal enum class ProtectedPairingCodeFailure(val code: String) {
    ACCESSIBILITY_UNAVAILABLE("accessibility_unavailable"),
    SETTINGS_HANDLER_UNAVAILABLE("settings_handler_unavailable"),
    SETTINGS_SURFACE_UNAVAILABLE("settings_surface_unavailable"),
    PAIRING_SURFACE_NOT_READY("pairing_surface_not_ready"),
    PAIRING_CODE_UNAVAILABLE("pairing_code_unavailable"),
    CHECKPOINT_OWNER_LOST("checkpoint_owner_lost"),
}

private fun ProtectedPairingCodeFailure.notReady() =
    PhoneControlProtectedCheckpointReadiness.NotReady(code)

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

private fun accessibilitySurfaceValues(
    root: AccessibilityNodeInfo,
): List<ProtectedPairingSurfaceValue> = accessibilityNodes(root).map { node ->
    ProtectedPairingSurfaceValue(
        text = node.text,
        resourceId = node.viewIdResourceName,
        editable = node.isEditable,
    )
}

internal fun protectedPairingSurfaceReady(
    values: List<ProtectedPairingSurfaceValue>,
): Boolean {
    val resourceEntries = values.mapNotNull { it.resourceId?.substringAfterLast('/') }
    val canonicalStructure =
        PAIRING_CODE_RESOURCE in resourceEntries &&
            PAIRING_CONTAINER_RESOURCE in resourceEntries &&
            PAIRING_ADDRESS_RESOURCE in resourceEntries
    val genericStructure =
        values.none(ProtectedPairingSurfaceValue::editable) &&
            values.any { isPairingEndpoint(it.text?.toString().orEmpty()) } &&
            hasUniqueOneTimeCode(values)
    return canonicalStructure || genericStructure
}

private fun hasUniqueOneTimeCode(values: List<ProtectedPairingSurfaceValue>): Boolean {
    var candidate: CharSequence? = null
    values.forEach { value ->
        val text = value.text ?: return@forEach
        if (text.length != ONE_TIME_CODE_LENGTH || text.any { it !in '0'..'9' }) return@forEach
        val prior = candidate
        if (prior != null && prior.toString() != text.toString()) return false
        candidate = text
    }
    return candidate != null
}

internal fun protectedPairingCode(
    values: List<ProtectedPairingSurfaceValue>,
): CharArray? {
    val code = uniqueAsciiOneTimeCode(
        values.filterNot(ProtectedPairingSurfaceValue::editable).map { it.text },
    ) ?: return null
    if (protectedPairingSurfaceReady(values)) return code
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

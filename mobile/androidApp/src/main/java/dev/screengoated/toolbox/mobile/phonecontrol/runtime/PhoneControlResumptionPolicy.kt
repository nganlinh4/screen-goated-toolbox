package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import java.nio.charset.StandardCharsets

internal object PhoneControlResumptionPolicy {
    const val MAXIMUM_HANDLE_UTF8_BYTES = 64 * 1024

    fun usableHandle(handle: String?): String? = handle?.takeIf {
        it.isNotBlank() && it.toByteArray(StandardCharsets.UTF_8).size <= MAXIMUM_HANDLE_UTF8_BYTES
    }
}

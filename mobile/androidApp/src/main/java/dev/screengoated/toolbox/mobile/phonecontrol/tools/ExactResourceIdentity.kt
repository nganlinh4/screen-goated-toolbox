package dev.screengoated.toolbox.mobile.phonecontrol.tools

import kotlinx.serialization.json.JsonObject

/** Platform-owned identity; a directory entry and its file object are distinct. */
internal interface ExactResourceIdentity : AutoCloseable {
    val hasFile: Boolean
    val description: JsonObject
    fun sameEntry(other: ExactResourceIdentity): Boolean
    fun sameFile(other: ExactResourceIdentity): Boolean
    fun whileReceiptMatches(sha256: String, credit: () -> Unit): Boolean
}

internal fun interface ExactResourceIdentitySource {
    fun capture(path: String): ExactResourceIdentity
}

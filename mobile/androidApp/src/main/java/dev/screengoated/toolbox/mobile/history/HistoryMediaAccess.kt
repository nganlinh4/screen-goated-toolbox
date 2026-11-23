package dev.screengoated.toolbox.mobile.history

import java.io.FileNotFoundException
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

internal data class HistoryMediaRequest(val id: Long, val name: String, val proof: HistoryMediaProof, val itemDigest: String)

internal fun historyItemDigest(item: HistoryItem): String = historyDigest(Json.encodeToString(item).toByteArray(Charsets.UTF_8))

private fun validDigest(value: String) = value.length == 64 && value.all { it in '0'..'9' || it in 'a'..'f' }

internal fun historyMediaRequest(parts: List<String>): HistoryMediaRequest {
    if (parts.size != 6) throw FileNotFoundException()
    val id = parts[0].toLongOrNull() ?: throw FileNotFoundException()
    val name = parts[1]
    if (!historyMediaLeaf(name)) throw FileNotFoundException()
    val identity = parts[2].split(':')
    if (identity.size != 2 || identity.any { it.toLongOrNull() == null }) throw FileNotFoundException()
    val size = parts[3].toLongOrNull()?.takeIf { it in 0..Int.MAX_VALUE.toLong() } ?: throw FileNotFoundException()
    val digest = parts[4]
    if (!validDigest(digest) || !validDigest(parts[5])) throw FileNotFoundException()
    return HistoryMediaRequest(id, name, HistoryMediaProof(parts[2], size, digest), parts[5])
}

internal fun authorizeHistoryMedia(state: HistoryUiState, request: HistoryMediaRequest) {
    if (state.storageUnavailable) throw FileNotFoundException()
    val item = state.items.filter { it.id == request.id }.singleOrNull() ?: throw FileNotFoundException()
    if (item.mediaPath != request.name || !historyMediaNameMatches(item.itemType, item.mediaPath) ||
        historyItemDigest(item) != request.itemDigest ||
        (item.mediaProof != null && item.mediaProof != request.proof)) throw FileNotFoundException()
}

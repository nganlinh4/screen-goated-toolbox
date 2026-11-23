package dev.screengoated.toolbox.mobile.creation

internal interface CreationSvgEditOperations {
    fun originalMatches(entry: CreationHistoryEntry): Boolean
    fun stagedMatches(receipt: CreationSvgEditReceipt): Boolean
    fun reserve(receipt: CreationSvgEditReceipt): CreationPendingReservation
    fun populate(receipt: CreationSvgEditReceipt)
    fun publish(receipt: CreationSvgEditReceipt): String
    fun publishedMatches(receipt: CreationSvgEditReceipt): Boolean
    fun persist(receipt: CreationSvgEditReceipt)
    fun complete(receipt: CreationSvgEditReceipt)
}

/** Advances a durable copy-on-write edit without modifying its original result. */
internal fun finishCreationSvgEdit(
    initial: CreationSvgEditReceipt,
    current: CreationHistoryEntry,
    operations: CreationSvgEditOperations,
    commit: (CreationHistoryEntry, CreationHistoryEntry) -> Unit,
): CreationHistoryEntry {
    var receipt = initial
    val old = receipt.entry
    val originalCopy = old.copy(id = receipt.originalCopyId, dispatchId = null)
    val published = receipt.publishedPath
    if (published != null && current.outputPath == published &&
        current.committedIdentity == receipt.pendingIdentity &&
        current.committedSize == receipt.size && current.committedSha256 == receipt.sha256
    ) {
        require(operations.publishedMatches(receipt)) { "Saved edit changed" }
        commit(current, originalCopy)
        runCatching { operations.complete(receipt) }
        return current
    }
    require(current == old && operations.originalMatches(old)) {
        "Saved result changed while editing"
    }
    require(operations.stagedMatches(receipt)) { "Saved edit is unavailable" }
    if (receipt.pendingHandle == null) {
        val reservation = operations.reserve(receipt)
        receipt = receipt.copy(pendingHandle = reservation.handle, pendingIdentity = reservation.identity)
        operations.persist(receipt)
    }
    if (!receipt.prepared) {
        operations.populate(receipt)
        receipt = receipt.copy(prepared = true)
        operations.persist(receipt)
    }
    if (receipt.publishedPath == null) {
        receipt = receipt.copy(publishedPath = operations.publish(receipt))
        operations.persist(receipt)
    }
    require(operations.publishedMatches(receipt)) { "Saved edit changed" }
    require(operations.originalMatches(old)) { "Saved result changed while editing" }
    val updated = withoutRetainedSvgOriginalMarker(old).copy(
        outputPath = requireNotNull(receipt.publishedPath),
        outputName = receipt.intent.finalName,
        createdAtMs = receipt.savedAtMs,
        committedSize = receipt.size,
        committedSha256 = receipt.sha256,
        committedIdentity = requireNotNull(receipt.pendingIdentity),
    )
    commit(updated, originalCopy)
    runCatching { operations.complete(receipt) }
    return updated
}

internal fun recoverCreationSvgEditEntries(
    entries: List<CreationHistoryEntry>,
    receipts: List<CreationSvgEditReceipt>,
    commitAll: (List<CreationHistoryEntry>) -> Unit,
    attempt: (CreationSvgEditReceipt, CreationHistoryEntry, (CreationHistoryEntry, CreationHistoryEntry) -> Unit) -> Unit,
): List<CreationHistoryEntry> {
    var current = entries
    receipts.forEach { receipt ->
        runCatching {
            val owner = requireCreationSvgEditOwner(current, receipt)
            attempt(receipt, owner) { updated, originalCopy ->
                val next = creationHistoryWithSvgCopy(current, updated, originalCopy)
                commitAll(next)
                current = next
            }
        }
    }
    return current
}

internal fun CreationNativeUiState.withSavedSvgEdit(
    previousPath: String,
    updated: CreationHistoryEntry,
    refreshedHistory: List<CreationHistoryEntry>?,
): CreationNativeUiState = copy(
    history = refreshedHistory ?: history.map { if (it.id == updated.id) updated else it },
    items = items.map { item ->
        val status = item.status
        if (status?.outputPath == previousPath && status.dispatchId == updated.dispatchId) {
            item.copy(status = status.copy(outputPath = updated.outputPath, outputName = updated.outputName))
        } else item
    },
)

package dev.screengoated.toolbox.mobile.creation

/**
 * Advances each delivery on its own so one unavailable destination never blocks the
 * others. A failed advance goes to [onFailure], which may settle the job terminally
 * (bounded retry) or return null to keep the delivery for the next round.
 */
internal fun reconcileCreationDeliveriesIndependently(
    records: List<CreationDeliveryRecord>,
    onFailure: (CreationDeliveryRecord, Throwable) -> CreationReconciledTerminal? = { _, _ -> null },
    advance: (CreationDeliveryRecord) -> CreationReconciledTerminal,
): Map<String, CreationReconciledTerminal> = buildMap {
    for (record in records) {
        val terminal = runCatching { advance(record) }.getOrElse { error ->
            runCatching { onFailure(record, error) }.getOrNull()
        }
        if (terminal != null) put(record.request.jobId, terminal)
    }
}

package dev.screengoated.toolbox.mobile.phonecontrol.provider.fileio

internal data class FileSnapshotMetadata(
    val size: Long,
    val modifiedSeconds: Long,
    val modifiedNanoseconds: Long,
    val changedSeconds: Long,
    val changedNanoseconds: Long,
)

internal fun boundedFileSnapshot(
    maximum: Int,
    metadata: () -> FileSnapshotMetadata,
    read: () -> ByteArray,
): ByteArray {
    require(maximum >= 0)
    val before = metadata()
    if (before.size > maximum) throw FileSnapshotLimitExceeded(before.size, maximum)
    val first = read()
    val second = read()
    val after = metadata()
    check(first.contentEquals(second) && first.size.toLong() == before.size && before == after) {
        "File changed during snapshot"
    }
    return first
}

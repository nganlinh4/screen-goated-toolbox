package dev.screengoated.toolbox.mobile.ui

internal fun closePresetRowSelection(selected: String?, owner: String): String? =
    if (selected == owner) null else selected

/** Retain structural selection across catalog updates, or choose its prior survivor. */
internal fun reconcilePresetRowSelection(
    previous: List<String>,
    current: List<String>,
    selected: String?,
): String? {
    if (selected == null) return null
    if (current.count { it == selected } == 1) return selected
    val index = previous.indexOf(selected)
    if (index < 0 || previous.lastIndexOf(selected) != index) return null
    val survivors = current.groupingBy { it }.eachCount().filterValues { it == 1 }.keys
    for (candidate in previous.subList(0, index).asReversed()) {
        if (candidate in survivors) return candidate
    }
    for (candidate in previous.drop(index + 1)) {
        if (candidate in survivors) return candidate
    }
    return null
}

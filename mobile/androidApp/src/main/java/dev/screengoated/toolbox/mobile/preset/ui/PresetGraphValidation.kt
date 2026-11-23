package dev.screengoated.toolbox.mobile.preset.ui

import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.Preset

/** Reject graph shapes that the editor cannot round-trip without losing data. */
internal fun Preset.graphIsEditable(): Boolean {
    val inputs = blocks.indices.filter { blocks[it].blockType == BlockType.INPUT_ADAPTER }
    if (inputs.size > 1 || (inputs.isNotEmpty() && inputs.single() != 0)) return false
    val seenEdges = mutableSetOf<Pair<Int, Int>>()
    val seenTargets = mutableSetOf<Int>()
    for ((from, to) in blockConnections) {
        if (from !in blocks.indices || to !in blocks.indices || from == to) return false
        if (blocks[to].blockType == BlockType.INPUT_ADAPTER) return false
        if (inputs.isEmpty() && to == 0) return false
        if (!seenEdges.add(from to to) || !seenTargets.add(to)) return false
    }
    return true
}

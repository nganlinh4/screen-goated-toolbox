package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.Preset

internal fun Preset.normalizedConnections(): List<Pair<Int, Int>> {
    if (blockConnections.isNotEmpty()) {
        return blockConnections.filter { (from, to) ->
            from in blocks.indices && to in blocks.indices
        }
    }

    if (blocks.size < 2) {
        return emptyList()
    }

    return (0 until blocks.lastIndex).map { index -> index to index + 1 }
}

internal fun PresetExecutionState.withWindowState(
    windowState: PresetResultWindowState,
): PresetExecutionState {
    val updated = resultWindows
        .filterNot { it.id == windowState.id }
        .plus(windowState)
        .sortedBy { it.overlayOrder }
    return copy(resultWindows = updated)
}

internal fun topologicalOrder(
    blockCount: Int,
    edges: List<Pair<Int, Int>>,
): List<Int> {
    val incomingCounts = IntArray(blockCount)
    val outgoing = MutableList(blockCount) { mutableListOf<Int>() }
    edges.forEach { (from, to) ->
        incomingCounts[to] += 1
        outgoing[from] += to
    }

    val ready = ArrayDeque<Int>()
    repeat(blockCount) { index ->
        if (incomingCounts[index] == 0) {
            ready += index
        }
    }

    val ordered = mutableListOf<Int>()
    while (ready.isNotEmpty()) {
        val next = ready.removeFirst()
        ordered += next
        outgoing[next].forEach { child ->
            incomingCounts[child] -= 1
            if (incomingCounts[child] == 0) {
                ready += child
            }
        }
    }

    if (ordered.size != blockCount) {
        error("Preset graph contains an unsupported cycle.")
    }

    return ordered
}

internal val supportedMarkdownRenderModes = setOf("markdown", "markdown_stream")

internal fun dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock.resolvePrompt(): String {
    var resolved = prompt
    languageVars.forEach { (key, value) ->
        resolved = resolved.replace("{$key}", value)
    }
    return resolved
}

internal fun dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock.requestsHtmlOutput(): Boolean {
    val normalizedPrompt = prompt.lowercase()
    return normalizedPrompt.contains("raw html")
        || normalizedPrompt.contains("standalone html")
        || normalizedPrompt.contains("html code")
        || normalizedPrompt.contains("```html")
}

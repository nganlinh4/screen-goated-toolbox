package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.BlockType

internal fun Preset.normalizedConnections(): List<Pair<Int, Int>> {
    if (graphExplicit || blockConnections.isNotEmpty()) {
        return blockConnections.filter { (from, to) ->
            from in blocks.indices && to in blocks.indices
        }
    }

    if (blocks.size < 2) {
        return emptyList()
    }

    return (0 until blocks.lastIndex).map { index -> index to index + 1 }
}

/** Select only blocks reachable from the canonical input root. */
internal fun Preset.reachableBlockIndices(edges: List<Pair<Int, Int>> = normalizedConnections()): List<Int> {
    if (blocks.isEmpty()) return emptyList()
    val outgoing = MutableList(blocks.size) { mutableListOf<Int>() }
    edges.forEach { (from, to) -> outgoing[from].add(to) }
    val seen = mutableSetOf(0)
    val queue = ArrayDeque<Int>()
    queue.add(0)
    while (queue.isNotEmpty()) {
        val current = queue.removeFirst()
        outgoing[current].forEach { if (seen.add(it)) queue.add(it) }
    }
    return seen.sorted()
}

internal fun Preset.reachableBlocks() = reachableBlockIndices().map { blocks[it] }

/** Audio branches that receive the original recording before processing. */
internal fun Preset.initialAudioBlockIndices(): List<Int> {
    val edges = normalizedConnections()
    val pending = ArrayDeque<Int>()
    val seen = mutableSetOf<Int>()
    val result = mutableListOf<Int>()
    if (blocks.isNotEmpty()) pending.add(0)
    while (pending.isNotEmpty()) {
        val index = pending.removeLast()
        if (!seen.add(index)) continue
        val block = blocks[index]
        when (block.blockType) {
            BlockType.AUDIO -> result.add(index)
            BlockType.INPUT_ADAPTER -> edges.asReversed().forEach { (from, to) ->
                if (from == index) pending.add(to)
            }
            else -> Unit
        }
    }
    return result
}

internal fun Preset.initialAudioBlocks() = initialAudioBlockIndices().map { blocks[it] }

internal fun Preset.precomputedAudioForBlock(index: Int, transcript: String?): String? =
    transcript?.takeIf { initialAudioBlockIndices().firstOrNull() == index }

/** Execute only blocks reachable from the canonical input root. */
internal fun Preset.reachableExecutionOrder(edges: List<Pair<Int, Int>>): List<Int> {
    val nodes = reachableBlockIndices(edges)
    val indices = nodes.withIndex().associate { it.value to it.index }
    val reachableEdges = edges.mapNotNull { (from, to) ->
        val left = indices[from] ?: return@mapNotNull null
        val right = indices[to] ?: return@mapNotNull null
        left to right
    }
    return topologicalOrder(nodes.size, reachableEdges).map { nodes[it] }
}

internal fun PresetExecutionState.withWindowState(
    windowState: PresetResultWindowState,
): PresetExecutionState {
    if (!isExecuting || sessionId != windowState.id.sessionId) return this
    val updated = resultWindows
        .filterNot { it.id == windowState.id }
        .plus(windowState)
        .sortedBy { it.overlayOrder }
    return copy(resultWindows = updated)
}

internal fun PresetExecutionState.withoutWindowState(
    id: PresetResultWindowId,
): PresetExecutionState {
    if (!isExecuting || sessionId != id.sessionId) return this
    return copy(resultWindows = resultWindows.filterNot { it.id == id })
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

internal fun PresetModelDescriptor.transportStreamingEnabled(): Boolean =
    provider != PresetModelProvider.GOOGLE_GTX &&
        provider != PresetModelProvider.TAALAS &&
        !(provider == PresetModelProvider.GROQ && fullName.startsWith("groq/compound"))

/**
 * Streaming is decided by render mode only, as on Windows. A streamed result
 * that becomes a complete raw HTML document is loaded once after the stream
 * ends (see rawDocumentLoad), never per chunk.
 */
internal fun dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock.surfaceStreamingEnabled(): Boolean =
    streamingEnabled && renderMode != "markdown"

internal fun dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock.resolvePrompt(): String {
    var resolved = prompt
    languageVars.forEach { (key, value) ->
        resolved = resolved.replace("{$key}", value)
    }
    return resolved
}

internal fun dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock.gtxTargetLanguage(): String? {
    return (languageVars["language1"] ?: languageVars["language"])
        ?.trim()
        ?.takeIf { it.isNotEmpty() }
}

package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.preset.ui.Connection
import dev.screengoated.toolbox.mobile.preset.ui.NodeGraphState
import dev.screengoated.toolbox.mobile.preset.ui.NodePosition
import dev.screengoated.toolbox.mobile.preset.ui.graphIsEditable
import dev.screengoated.toolbox.mobile.preset.ui.graphToPreset
import dev.screengoated.toolbox.mobile.preset.ui.presetToGraphState
import dev.screengoated.toolbox.mobile.preset.ui.canConnect
import dev.screengoated.toolbox.mobile.preset.ui.replaceIncomingConnection
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock
import java.nio.file.Files
import java.nio.file.Paths
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetGraphContractTest {
    private val json = Json { encodeDefaults = true }

    @Test
    fun sharedCasesKeepDraftsAndExecuteOnlyInputReachableBlocks() {
        val path = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "node-graph-editor.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "node-graph-editor.json"),
            Paths.get("parity-fixtures", "preset-system", "node-graph-editor.json"),
        ).first(Files::exists)
        val cases = json.parseToJsonElement(Files.readAllBytes(path).decodeToString()).jsonObject.getValue("cases").jsonArray
        cases.map { it.jsonObject }.filter { "blocks" in it }.forEach { case ->
            val names = case.getValue("blocks").jsonArray.map { it.jsonPrimitive.content }
            val blocks = names.map { name ->
                ProcessingBlock(name, if (name == "input") BlockType.INPUT_ADAPTER else BlockType.TEXT, "")
            }
            val edges = case.getValue("connections").jsonArray.map {
                it.jsonArray[0].jsonPrimitive.int to it.jsonArray[1].jsonPrimitive.int
            }
            val preset = sample(blocks, edges, case.getValue("graph_explicit").jsonPrimitive.boolean)
            val editable = case["expected_graph_editable"]?.jsonPrimitive?.boolean ?: true
            assertEquals(case.getValue("name").jsonPrimitive.content, editable, preset.graphIsEditable())
            case["expected_execution"]?.jsonArray?.let { expected ->
                val actual = preset.reachableExecutionOrder(preset.normalizedConnections()).map { blocks[it].id }
                assertEquals(expected.map { it.jsonPrimitive.content }, actual)
            }
            if (editable && names.firstOrNull() == "input") {
                val graph = NodeGraphState(
                    blocks.map { NodePosition(it.id, 0f, 0f, it) },
                    edges.map { Connection(names[it.first], names[it.second]) },
                )
                val saved = graphToPreset(graph, preset)
                assertTrue(saved.graphExplicit)
                assertEquals(case.getValue("expected_saved_blocks").jsonArray.map { it.jsonPrimitive.content }, saved.blocks.map { it.id })
            } else if (!editable) {
                assertEquals(preset, graphToPreset(NodeGraphState(emptyList(), emptyList()), preset))
                assertEquals(edges, preset.copy(nameEn = "edited").blockConnections)
            }
        }
    }

    @Test
    fun explicitFlagSurvivesFullAndDifferentialPersistence() {
        val legacy = sample(listOf(block("first"), block("second")), emptyList(), false)
        val explicit = legacy.copy(graphExplicit = true)
        assertFalse(json.decodeFromString<Preset>(json.encodeToString(legacy)).graphExplicit)
        assertTrue(json.decodeFromString<Preset>(json.encodeToString(explicit)).graphExplicit)
        val delta = explicit.toOverrideComparedTo(legacy)
        assertEquals(true, json.decodeFromString<PresetOverride>(json.encodeToString(delta)).graphExplicit)
        assertEquals(explicit, legacy.applyOverride(delta))
        assertEquals(explicit, legacy.applyOverride(explicit.toFullOverride()))
        assertEquals(listOf(0, 1), legacy.reachableExecutionOrder(legacy.normalizedConnections()))
        assertEquals(listOf(0), explicit.reachableExecutionOrder(explicit.normalizedConnections()))
    }

    @Test
    fun unrepresentableEdgesCannotBeRewrittenByEditor() {
        val input = ProcessingBlock("input", BlockType.INPUT_ADAPTER, "")
        val blocks = listOf(input, block("a"), block("b"))
        listOf(
            listOf(0 to 4), listOf(1 to 1), listOf(0 to 1, 0 to 1),
            listOf(0 to 2, 1 to 2), listOf(1 to 0),
        ).forEach { assertFalse(sample(blocks, it, true).graphIsEditable()) }
        assertFalse(sample(listOf(block("a"), input), emptyList(), true).graphIsEditable())
        assertFalse(sample(listOf(input, input), emptyList(), true).graphIsEditable())
        assertFalse(sample(listOf(block("a"), block("b")), listOf(1 to 0), true).graphIsEditable())
        assertTrue(sample(blocks, listOf(1 to 2, 2 to 1), true).graphIsEditable())
    }

    @Test
    fun disconnectedDraftDoesNotBlockCapabilityOrStartAudio() {
        val input = ProcessingBlock("input", BlockType.INPUT_ADAPTER, "")
        val draft = ProcessingBlock("draft", BlockType.AUDIO, "unavailable-model")
        val explicit = sample(listOf(input, draft), emptyList(), true)
        assertEquals(listOf(input), explicit.reachableBlocks())
        assertTrue(PresetExecutionCapabilityResolver().resolveExecutionCapability(explicit).supported)
        assertFalse(PresetExecutionCapabilityResolver().resolveExecutionCapability(explicit.copy(graphExplicit = false)).supported)
    }

    @Test
    fun disconnectedCycleDoesNotPreventTheReachableChain() {
        val input = ProcessingBlock("input", BlockType.INPUT_ADAPTER, "")
        val preset = sample(listOf(input, block("a"), block("b")), listOf(1 to 2, 2 to 1), true)
        assertEquals(listOf(0), preset.reachableExecutionOrder(preset.normalizedConnections()))
    }

    @Test
    fun initialAudioStopsAtProcessingBlocksAndPreservesBranchOrder() {
        val input = ProcessingBlock("input", BlockType.INPUT_ADAPTER, "")
        val text = block("text")
        val audioA = ProcessingBlock("a", BlockType.AUDIO, "")
        val audioB = ProcessingBlock("b", BlockType.AUDIO, "")
        val downstream = sample(listOf(input, text, audioA), listOf(0 to 1, 1 to 2), true)
        assertTrue(downstream.initialAudioBlocks().isEmpty())
        val branches = sample(listOf(input, audioA, audioB), listOf(0 to 2, 0 to 1), true)
        assertEquals(listOf(audioB, audioA), branches.initialAudioBlocks())
        assertEquals("streamed", branches.precomputedAudioForBlock(2, "streamed"))
        assertEquals(null, branches.precomputedAudioForBlock(1, "streamed"))
        assertEquals(null, downstream.precomputedAudioForBlock(2, "streamed"))
    }

    @Test
    fun connectingAnOccupiedInputReplacesOnlyItsPreviousWire() {
        val path = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "node-graph-editor.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "node-graph-editor.json"),
            Paths.get("parity-fixtures", "preset-system", "node-graph-editor.json"),
        ).first(Files::exists)
        val case = json.parseToJsonElement(Files.readAllBytes(path).decodeToString()).jsonObject
            .getValue("cases").jsonArray.map { it.jsonObject }
            .first { it.getValue("name").jsonPrimitive.content == "new_connection_replaces_occupied_input" }
        val nodes = listOf(
            NodePosition("input", 0f, 0f, ProcessingBlock("input", BlockType.INPUT_ADAPTER, "")),
            NodePosition("first", 0f, 0f, block("first")),
            NodePosition("target", 0f, 0f, block("target")),
        )
        fun edge(value: kotlinx.serialization.json.JsonElement): Connection {
            val pair = value.jsonArray
            return Connection(nodes[pair[0].jsonPrimitive.int].id, nodes[pair[1].jsonPrimitive.int].id)
        }
        val connections = case.getValue("initial_connections").jsonArray.map(::edge)
        val requested = edge(case.getValue("new_connection"))
        assertTrue(canConnect(nodes[1], nodes[2], connections))
        assertEquals(
            case.getValue("expected_connections").jsonArray.map(::edge),
            replaceIncomingConnection(connections, requested.fromNodeId, requested.toNodeId),
        )
        assertFalse(canConnect(nodes[2], nodes[2], connections))
        assertFalse(canConnect(nodes[1], nodes[0], connections))
    }

    @Test
    fun graphEditPreservesAuthoredBranchPriority() {
        val path = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "node-graph-editor.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "node-graph-editor.json"),
            Paths.get("parity-fixtures", "preset-system", "node-graph-editor.json"),
        ).first(Files::exists)
        val case = json.parseToJsonElement(Files.readAllBytes(path).decodeToString()).jsonObject
            .getValue("cases").jsonArray.map { it.jsonObject }
            .first { it.getValue("name").jsonPrimitive.content == "branch_priority_survives_node_edit" }
        val blocks = case.getValue("blocks").jsonArray.map {
            val id = it.jsonPrimitive.content
            ProcessingBlock(id, if (id == "input") BlockType.INPUT_ADAPTER else BlockType.AUDIO, "")
        }
        val connections = case.getValue("connections").jsonArray.map {
            it.jsonArray[0].jsonPrimitive.int to it.jsonArray[1].jsonPrimitive.int
        }
        val original = sample(blocks, connections, true)
        val graph = presetToGraphState(original)
        val updated = graph.copy(nodes = graph.nodes.map { it.copy(x = 10f) })
        val saved = graphToPreset(updated, original)
        assertEquals(case.getValue("expected_initial_branch").jsonPrimitive.content, saved.initialAudioBlocks().first().id)
        assertEquals(case.getValue("expected_saved_blocks").jsonArray.map { it.jsonPrimitive.content }, saved.blocks.map { it.id })
        assertEquals(connections, saved.blockConnections)
    }

    @Test
    fun duplicateBlockIdsRemainDistinctInTheEditor() {
        val first = block("same").copy(prompt = "first")
        val second = block("same").copy(prompt = "second")
        val preset = sample(listOf(first, second), emptyList(), false)
        val graph = presetToGraphState(preset)
        assertEquals(graph.nodes.size, graph.nodes.map { it.id }.toSet().size)
        val saved = graphToPreset(graph, preset)
        assertEquals(listOf(first, second), saved.blocks.drop(1))
        assertEquals(listOf(0 to 1, 1 to 2), saved.blockConnections)
        assertTrue(saved.graphExplicit)
    }

    private fun block(id: String) = ProcessingBlock(id, BlockType.TEXT, "")

    private fun sample(blocks: List<ProcessingBlock>, edges: List<Pair<Int, Int>>, explicit: Boolean) =
        Preset("test", "Test", "Test", "Test", PresetType.TEXT_INPUT, blocks,
            blockConnections = edges, graphExplicit = explicit)
}

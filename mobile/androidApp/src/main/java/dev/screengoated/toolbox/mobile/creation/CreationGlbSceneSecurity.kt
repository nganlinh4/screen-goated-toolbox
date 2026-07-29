package dev.screengoated.toolbox.mobile.creation

import kotlin.math.abs
import org.json.JSONObject

internal fun validateGlbSceneGraph(document: JSONObject, meshCosts: List<GlbMeshCost>) {
    val nodes = document.requiredArray("nodes", 1, CREATION_GLB_MAXIMUM_NODES)
    val incoming = IntArray(nodes.length())
    val hasMesh = BooleanArray(nodes.length())
    var instancedVertices = 0L
    var instancedIndices = 0L
    var instancedMorphElements = 0L
    val graph = List(nodes.length()) { index ->
        val node = nodes.requiredObject(index)
        require(!node.has("skin") && !node.has("camera")) {
            "The model result contains unsupported runtime work"
        }
        validateCreationGlbNodeTransform(node)
        if (node.has("mesh")) {
            hasMesh[index] = true
            val cost = meshCosts[node.requiredIndex("mesh", meshCosts.size)]
            instancedVertices = checkedAdd(instancedVertices, cost.vertices)
            instancedIndices = checkedAdd(instancedIndices, cost.indices)
            instancedMorphElements = checkedAdd(instancedMorphElements, cost.morphElements)
            require(
                instancedVertices <= CREATION_GLB_MAXIMUM_VERTICES &&
                    instancedIndices <= CREATION_GLB_MAXIMUM_INDICES &&
                    instancedMorphElements <= CREATION_GLB_MAXIMUM_MORPH_ELEMENTS
            ) { "The model result instantiates too much geometry" }
            validateCreationGlbWeights(node, cost.morphTargets)
        } else {
            require(!node.has("weights")) { "The model result has invalid morph weights" }
        }
        val children = node.optionalArray("children")
        require(children.length() <= MAXIMUM_GLB_CHILDREN_PER_NODE) {
            "The model result scene graph is too complex"
        }
        IntArray(children.length()) { child ->
            children.requiredIndex(child, nodes.length()).also {
                incoming[it] = Math.addExact(incoming[it], 1)
                require(incoming[it] == 1) { "The model result has a multi-parent node" }
            }
        }
    }
    require(graph.sumOf { it.size.toLong() } <= MAXIMUM_GLB_NODE_EDGES) {
        "The model result scene graph is too complex"
    }
    validateCreationGlbNodeDepth(graph)
    val scenes = document.requiredArray("scenes", 1, CREATION_GLB_MAXIMUM_SCENES)
    val selectedScene = if (document.has("scene")) {
        document.requiredIndex("scene", scenes.length())
    } else {
        0
    }
    val allRoots = mutableSetOf<Int>()
    var selectedHasGeometry = false
    var rootCount = 0
    repeat(scenes.length()) { sceneIndex ->
        val roots = scenes.requiredObject(sceneIndex).optionalArray("nodes")
        require(roots.length() <= CREATION_GLB_MAXIMUM_NODES) {
            "The model result scene is too complex"
        }
        if (sceneIndex == selectedScene) {
            require(roots.length() > 0) { "The selected model scene is empty" }
        }
        rootCount = Math.addExact(rootCount, roots.length())
        require(rootCount <= CREATION_GLB_MAXIMUM_NODES) {
            "The model result scene is too complex"
        }
        repeat(roots.length()) { root ->
            val node = roots.requiredIndex(root, nodes.length())
            require(incoming[node] == 0 && allRoots.add(node)) {
                "The model result has an invalid or repeated scene root"
            }
            if (sceneIndex == selectedScene) {
                val pending = ArrayDeque<Int>()
                pending.add(node)
                while (pending.isNotEmpty()) {
                    val current = pending.removeLast()
                    selectedHasGeometry = selectedHasGeometry || hasMesh[current]
                    graph[current].forEach(pending::add)
                }
            }
        }
    }
    require(selectedHasGeometry) { "The selected model scene has no geometry" }
}

internal fun validateCreationGlbWeights(container: JSONObject, expected: Int) {
    if (!container.has("weights")) return
    require(expected > 0) { "The model result has invalid morph weights" }
    val weights = container.requiredFiniteArray("weights", expected)
    require(weights.all { abs(it) <= CREATION_GLB_MAXIMUM_ABSOLUTE_RENDERER_VALUE }) {
        "The model result has excessive morph weights"
    }
}

private fun validateCreationGlbNodeTransform(node: JSONObject) {
    if (node.has("matrix")) {
        require(
            !node.has("translation") &&
                !node.has("rotation") &&
                !node.has("scale")
        ) { "The model result has conflicting node transforms" }
        validateBoundedArray(node, "matrix", 16)
    }
    listOf("translation" to 3, "rotation" to 4, "scale" to 3).forEach { (name, length) ->
        if (node.has(name)) validateBoundedArray(node, name, length)
    }
}

private fun validateBoundedArray(value: JSONObject, name: String, length: Int) {
    require(
        value.requiredFiniteArray(name, length)
            .all { abs(it) <= CREATION_GLB_MAXIMUM_ABSOLUTE_RENDERER_VALUE }
    ) { "The model result has an excessive node transform" }
}

private fun validateCreationGlbNodeDepth(graph: List<IntArray>) {
    val state = IntArray(graph.size)
    val depths = IntArray(graph.size)
    fun depth(index: Int, currentDepth: Int): Int {
        require(currentDepth <= CREATION_GLB_MAXIMUM_NODE_DEPTH && state[index] != 1) {
            "The model result scene graph is recursive or too deep"
        }
        if (state[index] == 2) return depths[index]
        state[index] = 1
        val result = 1 + (graph[index].maxOfOrNull { depth(it, currentDepth + 1) } ?: 0)
        require(result <= CREATION_GLB_MAXIMUM_NODE_DEPTH) {
            "The model result scene graph is too deep"
        }
        state[index] = 2
        depths[index] = result
        return result
    }
    graph.indices.forEach { depth(it, 1) }
}

private const val MAXIMUM_GLB_CHILDREN_PER_NODE = 1_024
private const val MAXIMUM_GLB_NODE_EDGES = 100_000L

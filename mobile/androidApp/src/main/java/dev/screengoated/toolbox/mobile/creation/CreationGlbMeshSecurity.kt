package dev.screengoated.toolbox.mobile.creation

import java.io.RandomAccessFile
import org.json.JSONObject

internal fun validateGlbMeshes(
    document: JSONObject,
    accessors: List<GlbAccessor>,
    input: RandomAccessFile,
    buffers: List<GlbBuffer>,
    views: List<GlbBufferView>,
): List<GlbMeshCost> {
    val meshes = document.requiredArray("meshes", 1, CREATION_GLB_MAXIMUM_MESHES)
    val materials = document.optionalArray("materials")
    require(materials.length() <= CREATION_GLB_MAXIMUM_MATERIALS) {
        "The model result has too many materials"
    }
    repeat(materials.length()) { materials.requiredObject(it) }
    val scanner = CreationGlbFloatScanner(input, buffers, views)
    val scannedFloatAccessors = mutableSetOf<Int>()
    val scannedPositionAccessors = mutableSetOf<Int>()
    var primitiveCount = 0
    var vertices = 0L
    var indices = 0L
    var morphTargets = 0
    var morphElements = 0L
    return List(meshes.length()) { meshIndex ->
        val mesh = meshes.requiredObject(meshIndex)
        val primitives = mesh.requiredArray("primitives", 1, CREATION_GLB_MAXIMUM_PRIMITIVES)
        primitiveCount = Math.addExact(primitiveCount, primitives.length())
        require(primitiveCount <= CREATION_GLB_MAXIMUM_PRIMITIVES) {
            "The model result has too many primitives"
        }
        var meshVertices = 0L
        var meshIndices = 0L
        var meshMorphElements = 0L
        var expectedTargets: Int? = null
        repeat(primitives.length()) { primitiveIndex ->
            val primitive = primitives.requiredObject(primitiveIndex)
            val attributes = requireNotNull(primitive.optJSONObject("attributes")) {
                "The model result has invalid geometry attributes"
            }
            require(attributes.length() in 1..CREATION_GLB_MAXIMUM_PRIMITIVE_ATTRIBUTES) {
                "The model result has invalid geometry attributes"
            }
            val positionIndex = attributes.requiredIndex("POSITION", accessors.size)
            val position = accessors[positionIndex]
            require(
                position.type == "VEC3" &&
                    position.componentType == GLB_FLOAT &&
                    position.absoluteOffset % 4L == 0L
            ) { "The model result has invalid position geometry" }
            if (scannedPositionAccessors.add(positionIndex)) {
                scanner.validate(position, containDeclaredBounds = true)
                scannedFloatAccessors += positionIndex
            }
            attributes.keys().forEach { semantic ->
                val accessorIndex = attributes.requiredIndex(semantic, accessors.size)
                val accessor = accessors[accessorIndex]
                require(
                    accessor.count == position.count &&
                        accessor.absoluteOffset % 4L == 0L &&
                        validCreationGlbVertexAttribute(semantic, accessor)
                ) { "The model result has invalid geometry attributes" }
                if (accessor.componentType == GLB_FLOAT && scannedFloatAccessors.add(accessorIndex)) {
                    scanner.validate(accessor, containDeclaredBounds = semantic == "POSITION")
                }
            }
            vertices = checkedAdd(vertices, position.count)
            meshVertices = checkedAdd(meshVertices, position.count)
            val mode = if (primitive.has("mode")) {
                primitive.requiredLong("mode", 0, 6).toInt()
            } else {
                4
            }
            require(mode == 4) { "The model result contains non-triangle geometry" }
            val indexCount = if (primitive.has("indices")) {
                val accessor = accessors[primitive.requiredIndex("indices", accessors.size)]
                require(
                    accessor.type == "SCALAR" &&
                        accessor.componentType in GLB_INDEX_COMPONENTS &&
                        views[accessor.view].stride == 0
                ) { "The model result has invalid primitive indices" }
                validateCreationGlbPrimitiveIndices(
                    input = input,
                    buffers = buffers,
                    views = views,
                    accessor = accessor,
                    positionCount = position.count,
                    mode = mode,
                )
                accessor.count
            } else {
                validateCreationGlbPrimitiveCount(position.count, mode)
                position.count
            }
            indices = checkedAdd(indices, indexCount)
            meshIndices = checkedAdd(meshIndices, indexCount)
            require(
                vertices <= CREATION_GLB_MAXIMUM_VERTICES &&
                    indices <= CREATION_GLB_MAXIMUM_INDICES
            ) { "The model result geometry is too complex" }
            if (primitive.has("material")) {
                primitive.requiredIndex("material", materials.length())
            }
            val targets = primitive.optionalArray("targets")
            require(expectedTargets == null || expectedTargets == targets.length()) {
                "The model result has inconsistent morph targets"
            }
            expectedTargets = targets.length()
            morphTargets = Math.addExact(morphTargets, targets.length())
            require(morphTargets <= CREATION_GLB_MAXIMUM_MORPH_TARGETS) {
                "The model result has too many morph targets"
            }
            repeat(targets.length()) { targetIndex ->
                val target = targets.requiredObject(targetIndex)
                require(target.length() in 1..CREATION_GLB_MAXIMUM_MORPH_ATTRIBUTES) {
                    "The model result has invalid morph geometry"
                }
                target.keys().forEach { semantic ->
                    require(semantic in GLB_MORPH_ATTRIBUTES) {
                        "The model result has invalid morph geometry"
                    }
                    val accessorIndex = target.requiredIndex(semantic, accessors.size)
                    val accessor = accessors[accessorIndex]
                    require(
                        accessor.type == "VEC3" &&
                            accessor.componentType == GLB_FLOAT &&
                            accessor.count == position.count &&
                            accessor.absoluteOffset % 4L == 0L
                    ) { "The model result has invalid morph geometry" }
                    if (semantic == "POSITION" && scannedPositionAccessors.add(accessorIndex)) {
                        scanner.validate(accessor, containDeclaredBounds = true)
                        scannedFloatAccessors += accessorIndex
                    } else if (
                        semantic != "POSITION" &&
                        scannedFloatAccessors.add(accessorIndex)
                    ) {
                        scanner.validate(accessor, containDeclaredBounds = false)
                    }
                    morphElements = checkedAdd(morphElements, accessor.count)
                    meshMorphElements = checkedAdd(meshMorphElements, accessor.count)
                    require(morphElements <= CREATION_GLB_MAXIMUM_MORPH_ELEMENTS) {
                        "The model result morph geometry is too complex"
                    }
                }
            }
        }
        val targetCount = expectedTargets ?: 0
        validateCreationGlbWeights(mesh, targetCount)
        GlbMeshCost(meshVertices, meshIndices, meshMorphElements, targetCount)
    }
}

private fun validCreationGlbVertexAttribute(
    semantic: String,
    accessor: GlbAccessor,
): Boolean = when (semantic) {
    "POSITION", "NORMAL" -> accessor.type == "VEC3" && accessor.componentType == GLB_FLOAT
    "TANGENT" -> accessor.type == "VEC4" && accessor.componentType == GLB_FLOAT
    "COLOR_0" -> accessor.type in setOf("VEC3", "VEC4") && accessor.isFloatOrNormalizedInteger()
    "TEXCOORD_0", "TEXCOORD_1", "TEXCOORD_2", "TEXCOORD_3" ->
        accessor.type == "VEC2" && accessor.isFloatOrNormalizedInteger()
    else -> semantic.startsWith("_") && accessor.componentCount <= 4
}

private fun GlbAccessor.isFloatOrNormalizedInteger(): Boolean =
    componentType == GLB_FLOAT ||
        normalized && componentType in setOf(5_120, 5_121, 5_122, 5_123)

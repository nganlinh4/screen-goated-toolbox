package dev.screengoated.toolbox.mobile.creation

import java.io.RandomAccessFile
import kotlin.math.abs
import org.json.JSONArray
import org.json.JSONObject

internal fun validateGlbSkins(
    document: JSONObject,
    accessors: List<GlbAccessor>,
    input: RandomAccessFile,
    buffers: List<GlbBuffer>,
    views: List<GlbBufferView>,
) {
    val nodes = document.requiredArray("nodes", 1, CREATION_GLB_MAXIMUM_NODES)
    val meshes = document.requiredArray("meshes", 1, CREATION_GLB_MAXIMUM_MESHES)
    if (!document.has("skins")) {
        rejectOrphanSkinWork(nodes, meshes)
        return
    }
    val skins = document.optionalArray("skins")
    require(skins.length() in 1..CREATION_GLB_MAXIMUM_SKINS) {
        "The model result has invalid skin metadata"
    }

    val reader = GlbSkinComponentReader(input, buffers, views)
    val jointCounts = IntArray(skins.length())
    var totalJoints = 0
    repeat(skins.length()) { skinIndex ->
        val skin = skins.requiredObject(skinIndex)
        val joints = skin.requiredArray("joints", 1, CREATION_GLB_MAXIMUM_JOINTS_PER_SKIN)
        totalJoints = Math.addExact(totalJoints, joints.length())
        require(totalJoints <= CREATION_GLB_MAXIMUM_TOTAL_JOINTS) {
            "The model result has too many skin joints"
        }
        val uniqueJoints = mutableSetOf<Int>()
        repeat(joints.length()) { jointIndex ->
            require(uniqueJoints.add(joints.requiredIndex(jointIndex, nodes.length()))) {
                "The model result has duplicate skin joints"
            }
        }
        if (skin.has("skeleton")) skin.requiredIndex("skeleton", nodes.length())
        if (skin.has("inverseBindMatrices")) {
            val accessor = accessors[skin.requiredIndex("inverseBindMatrices", accessors.size)]
            require(
                accessor.type == "MAT4" &&
                    accessor.componentType == GLB_FLOAT &&
                    accessor.count == joints.length().toLong() &&
                    !accessor.normalized
            ) { "The model result has invalid inverse-bind matrices" }
            reader.validateFiniteFloats(accessor)
        }
        jointCounts[skinIndex] = joints.length()
    }

    val referenced = BooleanArray(skins.length())
    val meshSkins = IntArray(meshes.length()) { -1 }
    repeat(nodes.length()) { nodeIndex ->
        val node = nodes.requiredObject(nodeIndex)
        if (node.has("skin")) {
            val skinIndex = node.requiredIndex("skin", skins.length())
            referenced[skinIndex] = true
            assignSkinScope(nodeIndex, skinIndex, nodes, meshSkins)
        }
    }
    require(referenced.all { it }) { "The model result contains an unused skin" }

    val scannedJoints = mutableSetOf<Pair<Int, Int>>()
    val scannedWeights = mutableSetOf<Int>()
    repeat(meshes.length()) { meshIndex ->
        val primitives = meshes.requiredObject(meshIndex)
            .requiredArray("primitives", 1, CREATION_GLB_MAXIMUM_PRIMITIVES)
        repeat(primitives.length()) { primitiveIndex ->
            val attributes = requireNotNull(
                primitives.requiredObject(primitiveIndex).optJSONObject("attributes"),
            ) { "The model result has invalid skin attributes" }
            val hasJoints = attributes.has("JOINTS_0")
            val hasWeights = attributes.has("WEIGHTS_0")
            require(hasJoints == hasWeights) { "The model result has incomplete skin attributes" }
            if (!hasJoints) {
                require(meshSkins[meshIndex] < 0) {
                    "The model result has an unweighted skinned primitive"
                }
                return@repeat
            }
            val skinIndex = meshSkins[meshIndex]
            require(skinIndex >= 0) { "The model result has orphaned skin attributes" }
            val jointIndex = attributes.requiredIndex("JOINTS_0", accessors.size)
            val weightIndex = attributes.requiredIndex("WEIGHTS_0", accessors.size)
            val joints = accessors[jointIndex]
            val weights = accessors[weightIndex]
            require(
                joints.type == "VEC4" &&
                    joints.componentType in setOf(5_121, 5_123) &&
                    !joints.normalized &&
                    weights.type == "VEC4" &&
                    (weights.componentType == GLB_FLOAT ||
                        weights.normalized && weights.componentType in setOf(5_121, 5_123)) &&
                    joints.count == weights.count
            ) { "The model result has invalid skin attributes" }
            val jointCount = jointCounts[skinIndex]
            if (scannedJoints.add(jointIndex to jointCount)) {
                reader.validateJoints(joints, jointCount)
            }
            if (scannedWeights.add(weightIndex)) reader.validateWeights(weights)
        }
    }
}

private fun rejectOrphanSkinWork(nodes: JSONArray, meshes: JSONArray) {
    repeat(nodes.length()) { index ->
        require(!nodes.requiredObject(index).has("skin")) {
            "The model result has an orphaned skin reference"
        }
    }
    repeat(meshes.length()) { meshIndex ->
        val primitives = meshes.requiredObject(meshIndex)
            .requiredArray("primitives", 1, CREATION_GLB_MAXIMUM_PRIMITIVES)
        repeat(primitives.length()) { primitiveIndex ->
            val attributes = requireNotNull(
                primitives.requiredObject(primitiveIndex).optJSONObject("attributes"),
            ) { "The model result has invalid geometry attributes" }
            require(!attributes.has("JOINTS_0") && !attributes.has("WEIGHTS_0")) {
                "The model result has orphaned skin attributes"
            }
        }
    }
}

private fun assignSkinScope(
    root: Int,
    skin: Int,
    nodes: JSONArray,
    meshSkins: IntArray,
) {
    val pending = ArrayDeque<Int>().apply { add(root) }
    var foundGeometry = false
    while (pending.isNotEmpty()) {
        val nodeIndex = pending.removeLast()
        val node = nodes.requiredObject(nodeIndex)
        require(nodeIndex == root || !node.has("skin")) {
            "The model result has nested skin scopes"
        }
        if (node.has("mesh")) {
            val mesh = node.requiredIndex("mesh", meshSkins.size)
            require(meshSkins[mesh] < 0 || meshSkins[mesh] == skin) {
                "The model result has ambiguous skin scopes"
            }
            meshSkins[mesh] = skin
            foundGeometry = true
        }
        val children = node.optionalArray("children")
        repeat(children.length()) { index ->
            pending.add(children.requiredIndex(index, nodes.length()))
        }
    }
    require(foundGeometry) { "The model result has an empty skin scope" }
}

private class GlbSkinComponentReader(
    private val input: RandomAccessFile,
    private val buffers: List<GlbBuffer>,
    private val views: List<GlbBufferView>,
) {
    private val pages = mutableMapOf<Int, Page>()

    fun validateFiniteFloats(accessor: GlbAccessor) {
        repeatElements(accessor) { offset ->
            val value = readFloat(accessor, offset)
            require(value.isFinite() && abs(value) <= CREATION_GLB_MAXIMUM_ABSOLUTE_RENDERER_VALUE) {
                "The model result has invalid inverse-bind matrices"
            }
        }
    }

    fun validateJoints(accessor: GlbAccessor, jointCount: Int) {
        repeatElements(accessor) { offset ->
            require(readUnsigned(accessor, offset) < jointCount) {
                "The model result has an out-of-range skin joint"
            }
        }
    }

    fun validateWeights(accessor: GlbAccessor) {
        repeat(accessor.count.toInt()) { element ->
            var sum = 0.0
            repeat(4) { component ->
                val offset = componentOffset(accessor, element.toLong(), component)
                val value = when (accessor.componentType) {
                    5_121 -> readUnsigned(accessor, offset).toDouble() / UByte.MAX_VALUE.toDouble()
                    5_123 -> readUnsigned(accessor, offset).toDouble() / UShort.MAX_VALUE.toDouble()
                    GLB_FLOAT -> readFloat(accessor, offset)
                    else -> error("The model result has invalid skin weights")
                }
                require(value.isFinite() && value in 0.0..1.0) {
                    "The model result has invalid skin weights"
                }
                sum += value
            }
            require(abs(sum - 1.0) <= SKIN_WEIGHT_SUM_TOLERANCE) {
                "The model result has unnormalized skin weights"
            }
        }
    }

    private inline fun repeatElements(accessor: GlbAccessor, action: (Long) -> Unit) {
        repeat(accessor.count.toInt()) { element ->
            repeat(accessor.componentCount) { component ->
                action(componentOffset(accessor, element.toLong(), component))
            }
        }
    }

    private fun componentOffset(accessor: GlbAccessor, element: Long, component: Int): Long {
        val componentBytes = if (accessor.componentType == 5_121) 1 else if (
            accessor.componentType == 5_123
        ) 2 else 4
        val stride = if (accessor.stride == 0) accessor.elementBytes else accessor.stride
        return checkedAdd(
            accessor.absoluteOffset,
            checkedAdd(
                checkedMultiply(element, stride.toLong()),
                checkedMultiply(component.toLong(), componentBytes.toLong()),
            ),
        )
    }

    private fun readUnsigned(accessor: GlbAccessor, offset: Long): Int {
        val bytes = read(accessor, offset, if (accessor.componentType == 5_121) 1 else 2)
        return if (bytes.size == 1) {
            bytes[0].toInt() and 0xff
        } else {
            (bytes[0].toInt() and 0xff) or ((bytes[1].toInt() and 0xff) shl 8)
        }
    }

    private fun readFloat(accessor: GlbAccessor, offset: Long): Double {
        val bytes = read(accessor, offset, 4)
        val bits = (bytes[0].toInt() and 0xff) or
            ((bytes[1].toInt() and 0xff) shl 8) or
            ((bytes[2].toInt() and 0xff) shl 16) or
            ((bytes[3].toInt() and 0xff) shl 24)
        return Float.fromBits(bits).toDouble()
    }

    private fun read(accessor: GlbAccessor, offset: Long, length: Int): ByteArray {
        val view = views[accessor.view]
        val buffer = buffers[view.buffer]
        require(offset >= 0 && checkedAdd(offset, length.toLong()) <= buffer.length) {
            "The model result skin data exceeds its buffer"
        }
        buffer.embedded?.let { return it.copyOfRange(offset.toInt(), offset.toInt() + length) }
        val page = pages.getOrPut(view.buffer) { Page() }
        if (offset < page.start || checkedAdd(offset, length.toLong()) > page.start + page.length) {
            val binary = requireNotNull(buffer.binary)
            page.start = offset
            page.length = minOf(SKIN_PAGE_BYTES.toLong(), buffer.length - offset).toInt()
            input.seek(checkedAdd(binary.offset, offset))
            input.readFully(page.bytes, 0, page.length)
        }
        val start = (offset - page.start).toInt()
        return page.bytes.copyOfRange(start, start + length)
    }

    private class Page {
        val bytes = ByteArray(SKIN_PAGE_BYTES)
        var start = -1L
        var length = 0
    }
}

private const val SKIN_WEIGHT_SUM_TOLERANCE = 0.01
private const val SKIN_PAGE_BYTES = 64 * 1024

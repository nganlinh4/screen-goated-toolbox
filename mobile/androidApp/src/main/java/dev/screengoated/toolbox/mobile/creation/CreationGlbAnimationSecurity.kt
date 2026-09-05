package dev.screengoated.toolbox.mobile.creation

import java.io.RandomAccessFile
import kotlin.math.abs
import org.json.JSONArray
import org.json.JSONObject

internal const val CREATION_GLB_MAXIMUM_ANIMATION_CLIPS = 64
internal const val CREATION_GLB_MAXIMUM_ANIMATION_CHANNELS = 2048
internal const val CREATION_GLB_MAXIMUM_ANIMATION_KEYS = 1_000_000L
internal const val CREATION_GLB_MAXIMUM_ANIMATION_COMPONENTS = 6_000_000L
internal const val CREATION_GLB_MAXIMUM_ANIMATION_DURATION = 3600.0

internal fun validateGlbAnimations(
    document: JSONObject,
    accessors: List<GlbAccessor>,
    input: RandomAccessFile,
    buffers: List<GlbBuffer>,
    views: List<GlbBufferView>,
) {
    if (!document.has("animations")) return
    val clips = document.optionalArray("animations")
    require(clips.length() <= CREATION_GLB_MAXIMUM_ANIMATION_CLIPS)
    val nodes = document.optionalArray("nodes")
    val reader = GlbComponentReader(input, buffers, views)
    var channelCost = 0L
    var keyCost = 0L
    var componentCost = 0L
    repeat(clips.length()) { clipIndex ->
        val clip = clips.requiredObject(clipIndex)
        val samplers = clip.requiredArray("samplers", 1, CREATION_GLB_MAXIMUM_ANIMATION_CHANNELS)
        val channels = clip.requiredArray("channels", 1, CREATION_GLB_MAXIMUM_ANIMATION_CHANNELS)
        channelCost += channels.length()
        require(channelCost <= CREATION_GLB_MAXIMUM_ANIMATION_CHANNELS)
        val targets = mutableSetOf<Pair<Int, String>>()
        val used = mutableSetOf<Int>()
        repeat(channels.length()) { channelIndex ->
            val channel = channels.requiredObject(channelIndex)
            val samplerIndex = channel.requiredIndex("sampler", samplers.length())
            used.add(samplerIndex)
            val sampler = samplers.requiredObject(samplerIndex)
            val target = requireNotNull(channel.opt("target") as? JSONObject)
            val nodeIndex = target.requiredIndex("node", nodes.length())
            val node = nodes.requiredObject(nodeIndex)
            val path = requireNotNull(target.opt("path") as? String)
            require(targets.add(nodeIndex to path))
            val width = when (path) {
                "translation", "scale" -> 3L
                "rotation" -> 4L
                "weights" -> animationMorphWidth(document, node)
                else -> error("The model result has an unsupported animation target")
            }
            require(path == "weights" || !node.has("matrix"))
            val timeAccessor = accessors[sampler.requiredIndex("input", accessors.size)]
            val outputAccessor = accessors[sampler.requiredIndex("output", accessors.size)]
            val interpolation = if (sampler.has("interpolation")) {
                requireNotNull(sampler.opt("interpolation") as? String)
            } else "LINEAR"
            val factor = when (interpolation) {
                "LINEAR", "STEP" -> 1L
                "CUBICSPLINE" -> 3L
                else -> error("The model result has unsupported animation interpolation")
            }
            val outputWidth = if (path == "weights") 1L else width
            require(timeAccessor.count > 0 && timeAccessor.componentType == GLB_FLOAT)
            require(timeAccessor.componentCount == 1 && !timeAccessor.normalized && timeAccessor.stride == 0)
            require(outputAccessor.componentType == GLB_FLOAT && outputAccessor.componentCount.toLong() == outputWidth)
            require(!outputAccessor.normalized && outputAccessor.stride == 0)
            keyCost = checkedAdd(keyCost, timeAccessor.count)
            val count = checkedMultiply(checkedMultiply(timeAccessor.count, width), factor)
            componentCost = checkedAdd(componentCost, count)
            require(keyCost <= CREATION_GLB_MAXIMUM_ANIMATION_KEYS)
            require(componentCost <= CREATION_GLB_MAXIMUM_ANIMATION_COMPONENTS)
            require(checkedMultiply(outputAccessor.count, outputWidth) == count)
            fun output(key: Long, component: Long, slot: Long): Double = boundedAnimationValue(
                reader.readPackedFloat(outputAccessor, (key * factor + slot) * width + component),
            )
            var previous: Double? = null
            repeat(timeAccessor.count.toInt()) { keyIndex ->
                val key = keyIndex.toLong()
                val time = boundedAnimationValue(reader.readPackedFloat(timeAccessor, key))
                require(time in 0.0..CREATION_GLB_MAXIMUM_ANIMATION_DURATION)
                previous?.let { require(time > it) }
                var quaternionLength = 0.0
                repeat(width.toInt()) { componentIndex ->
                    val component = componentIndex.toLong()
                    val value = output(key, component, if (factor == 3L) 1 else 0)
                    quaternionLength += value * value
                    if (factor == 3L) {
                        val incoming = output(key, component, 0)
                        output(key, component, 2)
                        previous?.let { previousTime ->
                            val interval = time - previousTime
                            boundedAnimationValue(output(key - 1, component, 1) + output(key - 1, component, 2) * interval / 3)
                            boundedAnimationValue(value - incoming * interval / 3)
                        }
                    }
                }
                require(path != "rotation" || abs(quaternionLength - 1.0) <= 0.02)
                previous = time
            }
        }
        require(used.size == samplers.length())
    }
}

private fun animationMorphWidth(document: JSONObject, node: JSONObject): Long {
    val meshes = document.optionalArray("meshes")
    val mesh = meshes.requiredObject(node.requiredIndex("mesh", meshes.length()))
    val primitives = mesh.optionalArray("primitives")
    require(primitives.length() > 0)
    var width: Long? = null
    repeat(primitives.length()) { index ->
        val targets = requireNotNull(primitives.requiredObject(index).opt("targets") as? JSONArray)
        val count = targets.length().toLong()
        require(count in 1..256 && (width == null || width == count))
        width = count
    }
    return requireNotNull(width)
}

private fun boundedAnimationValue(value: Double): Double {
    require(value.isFinite() && abs(value) <= CREATION_GLB_MAXIMUM_ABSOLUTE_RENDERER_VALUE) {
        "The model result contains invalid animation data"
    }
    return value
}

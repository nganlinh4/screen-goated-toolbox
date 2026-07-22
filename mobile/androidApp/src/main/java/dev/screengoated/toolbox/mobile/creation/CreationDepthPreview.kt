package dev.screengoated.toolbox.mobile.creation

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.RectF
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.nativeCanvas
import java.io.File
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private sealed interface DepthPreviewFrame {
    fun recycle()

    data class Relief(
        val source: Bitmap,
        val depthSamples: FloatArray,
        val vertices: FloatArray = FloatArray(RELIEF_VERTEX_COUNT * 2),
        val paint: Paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG),
    ) : DepthPreviewFrame {
        override fun recycle() {
            source.recycle()
        }
    }

    data class Layers(
        val bitmaps: List<Bitmap>,
        val width: Int,
        val height: Int,
    ) : DepthPreviewFrame {
        override fun recycle() = bitmaps.forEach(Bitmap::recycle)
    }
}

@Composable
internal fun CreationDepthPreview(tool: CreationTool, sourcePath: String, depthPath: String) {
    val frame by produceState<DepthPreviewFrame?>(null, tool, sourcePath, depthPath) {
        value = null
        val loaded = withContext(Dispatchers.IO) {
            runCatching { loadDepthFrame(tool, sourcePath, depthPath) }.getOrNull()
        }
        value = loaded
    }
    val active = frame
    DisposableEffect(active) {
        onDispose { active?.recycle() }
    }
    if (active == null) {
        CreationImageThumbnail(sourcePath, Modifier.fillMaxSize())
        return
    }
    val transition = rememberInfiniteTransition(label = "creation-depth-preview")
    val phase by transition.animateFloat(
        initialValue = 0f,
        targetValue = (PI * 2).toFloat(),
        animationSpec = infiniteRepeatable(tween(4_800), RepeatMode.Restart),
        label = "depth-phase",
    )
    Canvas(Modifier.fillMaxSize()) {
        when (active) {
            is DepthPreviewFrame.Relief -> drawRelief(active, phase)
            is DepthPreviewFrame.Layers -> drawLayers(active, phase)
        }
    }
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawRelief(
    frame: DepthPreviewFrame.Relief,
    phase: Float,
) {
    val target = fitRect(frame.source.width, frame.source.height, size.width, size.height, 0.88f)
    val yaw = sin(phase) * 0.34f
    val yawCos = cos(yaw)
    val yawSin = sin(yaw)
    var offset = 0
    var sampleIndex = 0
    for (row in 0..RELIEF_MESH_HEIGHT) {
        val vertical = row.toFloat() / RELIEF_MESH_HEIGHT
        val normalizedY = vertical - 0.5f
        for (column in 0..RELIEF_MESH_WIDTH) {
            val horizontal = column.toFloat() / RELIEF_MESH_WIDTH
            val normalizedX = horizontal - 0.5f
            val depth = frame.depthSamples[sampleIndex++]
            val rotatedX = normalizedX * yawCos + depth * yawSin * 0.46f
            val rotatedZ = -normalizedX * yawSin + depth * yawCos * 0.46f
            val perspective = 1f / (1f + rotatedZ * 0.32f)
            frame.vertices[offset++] = target.centerX() + rotatedX * target.width() * perspective
            frame.vertices[offset++] = target.centerY() +
                (normalizedY * target.height() + depth * target.height() * 0.025f) * perspective +
                sin(phase * 1.35f) * target.height() * 0.008f
        }
    }
    drawContext.canvas.nativeCanvas.drawBitmapMesh(
        frame.source,
        RELIEF_MESH_WIDTH,
        RELIEF_MESH_HEIGHT,
        frame.vertices,
        0,
        null,
        0,
        frame.paint,
    )
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawLayers(
    frame: DepthPreviewFrame.Layers,
    phase: Float,
) {
    val target = fitRect(frame.width, frame.height, size.width, size.height, 0.88f)
    val pulse = 0.48f + sin(phase) * 0.24f
    val spread = min(target.width(), target.height()) * 0.065f * pulse
    val source = Rect(0, 0, frame.width, frame.height)
    val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    frame.bitmaps.forEachIndexed { index, layer ->
        val depthPosition = index.toFloat() / (frame.bitmaps.size - 1) - 0.5f
        val offsetX = depthPosition * spread * 1.7f
        val offsetY = -depthPosition * spread * 0.48f
        val scale = 1f + depthPosition * 0.035f * pulse
        val canvas = drawContext.canvas.nativeCanvas
        canvas.save()
        canvas.translate(target.centerX() + offsetX, target.centerY() + offsetY)
        canvas.scale(scale, scale)
        canvas.drawBitmap(
            layer,
            source,
            RectF(-target.width() / 2f, -target.height() / 2f, target.width() / 2f, target.height() / 2f),
            paint,
        )
        canvas.restore()
    }
}

private fun loadDepthFrame(
    tool: CreationTool,
    sourcePath: String,
    depthPath: String,
): DepthPreviewFrame {
    val source = decodeSampled(sourcePath, MAX_PREVIEW_SIDE)
    val depth = try {
        requireNotNull(BitmapFactory.decodeFile(depthPath)) { "Could not read depth map" }
    } catch (error: Throwable) {
        source.recycle()
        throw error
    }
    if (tool == CreationTool.IMAGE_TO_3D) {
        return try {
            DepthPreviewFrame.Relief(source, sampleReliefDepth(depth))
        } catch (error: Throwable) {
            source.recycle()
            throw error
        } finally {
            depth.recycle()
        }
    }
    return try {
        DepthPreviewFrame.Layers(
            bitmaps = buildDepthLayers(source, depth),
            width = source.width,
            height = source.height,
        )
    } finally {
        source.recycle()
        depth.recycle()
    }
}

private fun sampleReliefDepth(depth: Bitmap): FloatArray = FloatArray(RELIEF_VERTEX_COUNT).also {
    var index = 0
    for (row in 0..RELIEF_MESH_HEIGHT) {
        val y = row * (depth.height - 1) / RELIEF_MESH_HEIGHT
        for (column in 0..RELIEF_MESH_WIDTH) {
            val x = column * (depth.width - 1) / RELIEF_MESH_WIDTH
            it[index++] = ((depth.getPixel(x, y) and 0xff) / 255f - 0.5f) * 0.9f
        }
    }
}

private fun buildDepthLayers(source: Bitmap, depth: Bitmap): List<Bitmap> {
    val width = source.width
    val height = source.height
    val sourcePixels = IntArray(width * height)
    source.getPixels(sourcePixels, 0, width, 0, 0, width, height)
    val layerPixels = Array(DEPTH_BIN_COUNT) { IntArray(sourcePixels.size) }
    sourcePixels.forEachIndexed { index, pixel ->
        val x = index % width
        val y = index / width
        val depthX = x * depth.width / width
        val depthY = y * depth.height / height
        val value = depth.getPixel(depthX, depthY) and 0xff
        val bin = (value * DEPTH_BIN_COUNT / 256).coerceAtMost(DEPTH_BIN_COUNT - 1)
        layerPixels[bin][index] = pixel
    }
    return layerPixels.map { pixels ->
        Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888)
    }
}

private fun decodeSampled(path: String, maximumSide: Int): Bitmap {
    require(File(path).isFile) { "Preview source is unavailable" }
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(path, bounds)
    val largest = maxOf(bounds.outWidth, bounds.outHeight).coerceAtLeast(1)
    val sample = Integer.highestOneBit((largest / maximumSide).coerceAtLeast(1))
    return requireNotNull(
        BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample }),
    ) { "Could not decode preview source" }
}

private fun fitRect(
    sourceWidth: Int,
    sourceHeight: Int,
    availableWidth: Float,
    availableHeight: Float,
    fill: Float,
): RectF {
    val scale = min(
        availableWidth * fill / sourceWidth.coerceAtLeast(1),
        availableHeight * fill / sourceHeight.coerceAtLeast(1),
    )
    val width = sourceWidth * scale
    val height = sourceHeight * scale
    val left = (availableWidth - width) / 2f
    val top = (availableHeight - height) / 2f
    return RectF(left, top, left + width, top + height)
}

private const val DEPTH_BIN_COUNT = 6
private const val MAX_PREVIEW_SIDE = 720
private const val RELIEF_MESH_WIDTH = 28
private const val RELIEF_MESH_HEIGHT = 28
private const val RELIEF_VERTEX_COUNT = (RELIEF_MESH_WIDTH + 1) * (RELIEF_MESH_HEIGHT + 1)

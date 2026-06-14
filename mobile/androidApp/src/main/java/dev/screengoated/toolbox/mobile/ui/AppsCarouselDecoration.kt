@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.toPath
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Matrix
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.RoundedPolygon
import dev.screengoated.toolbox.mobile.ui.theme.SgtExtendedColors

internal data class AppSlot(val shape: RoundedPolygon, val colorToken: (SgtExtendedColors) -> Color)

private data class ShapeInstance(
    val shape: RoundedPolygon,
    val xFrac: Float, val yFrac: Float,
    val sizeFrac: Float, val alpha: Float,
    val rotation: Float,
)

private val allDecoShapes by lazy { listOf(
    MaterialShapes.Arch, MaterialShapes.Arrow, MaterialShapes.Boom, MaterialShapes.Bun,
    MaterialShapes.Burst, MaterialShapes.Circle, MaterialShapes.ClamShell,
    MaterialShapes.Clover4Leaf, MaterialShapes.Clover8Leaf,
    MaterialShapes.Cookie12Sided, MaterialShapes.Cookie4Sided, MaterialShapes.Cookie6Sided,
    MaterialShapes.Cookie7Sided, MaterialShapes.Cookie9Sided,
    MaterialShapes.Diamond, MaterialShapes.Fan, MaterialShapes.Flower, MaterialShapes.Gem,
    MaterialShapes.Ghostish, MaterialShapes.Heart, MaterialShapes.Oval, MaterialShapes.Pentagon,
    MaterialShapes.Pill, MaterialShapes.PixelCircle, MaterialShapes.PixelTriangle,
    MaterialShapes.Puffy, MaterialShapes.PuffyDiamond, MaterialShapes.SemiCircle,
    MaterialShapes.Slanted, MaterialShapes.SoftBoom, MaterialShapes.SoftBurst,
    MaterialShapes.Square, MaterialShapes.Sunny, MaterialShapes.Triangle, MaterialShapes.VerySunny,
) }

/** Place shapes with collision detection — no overlapping. */
private fun generateNonOverlappingShapes(seed: Long): List<ShapeInstance> {
    val rng = java.util.Random(seed)
    val placed = mutableListOf<ShapeInstance>()
    var attempts = 0
    while (placed.size < 6 && attempts < 80) {
        attempts++
        val sizeFrac = 0.15f + rng.nextFloat() * 0.75f // tiny to huge
        val xFrac = -0.05f + rng.nextFloat() * 1.10f   // allow overflow left/right
        val yFrac = -0.10f + rng.nextFloat() * 1.20f    // allow overflow top/bottom
        val collides = placed.any { other ->
            val dx = xFrac - other.xFrac
            val dy = yFrac - other.yFrac
            val minDist = (sizeFrac + other.sizeFrac) * 0.32f
            dx * dx + dy * dy < minDist * minDist
        }
        if (!collides) {
            placed.add(ShapeInstance(
                shape = allDecoShapes[rng.nextInt(allDecoShapes.size)],
                xFrac = xFrac, yFrac = yFrac,
                sizeFrac = sizeFrac,
                alpha = 0.10f + rng.nextFloat() * 0.16f,
                rotation = rng.nextFloat() * 360f,
            ))
        }
    }
    return placed
}

/**
 * Non-overlapping shapes with smooth morphing + slight spin + spring bounce.
 * Each shape periodically morphs to another MaterialShape via Morph(A,B).toPath(progress).
 * During the morph, a slight rotation is applied (spring bounce).
 * Idle: morph every 3-6s. Scrolling/active: morph every 0.8-1.6s.
 */
@Composable
internal fun AnimatedShapesCanvas(
    color: Color,
    seed: Long,
    modifier: Modifier = Modifier,
    isScrolling: Boolean = false,
) {
    val placements = remember(seed) { generateNonOverlappingShapes(seed) }

    // Per-shape morph state: tracks the current from→to morph pair + generation counter
    // The generation counter drives the animateFloatAsState target flip (0f↔1f)
    data class MorphPair(
        val from: RoundedPolygon,
        val to: RoundedPolygon,
        val gen: Int,
        val spinDelta: Float,
    )

    @Composable
    fun rememberAnimatedShape(i: Int, inst: ShapeInstance): Triple<Morph, Float, Float> {
        var pair by remember { mutableStateOf(MorphPair(inst.shape, inst.shape, 0, 0f)) }

        val intervalMs = if (isScrolling) (800L + i * 200L) else (3000L + i * 1500L)
        LaunchedEffect(isScrolling, i) {
            val rng = java.util.Random(seed + i * 17L)
            while (true) {
                kotlinx.coroutines.delay(intervalMs)
                val nextShape = allDecoShapes[rng.nextInt(allDecoShapes.size)]
                val spinDelta = (rng.nextFloat() - 0.5f) * 30f // ±15° spin during morph
                pair = MorphPair(pair.to, nextShape, pair.gen + 1, spinDelta)
            }
        }

        // Morph progress: animate 0→1 each time gen changes (odd→1, even→0)
        val morphTarget = if (pair.gen % 2 == 0) 0f else 1f
        val morphProgress by animateFloatAsState(
            targetValue = morphTarget,
            animationSpec = spring(
                dampingRatio = Spring.DampingRatioNoBouncy,
                stiffness = Spring.StiffnessVeryLow,
            ),
            label = "morph-$i",
        )
        // Actual progress within the current pair: how far from→to
        val t = if (pair.gen % 2 == 0) (1f - morphProgress) else morphProgress

        // Spin: slight rotation during morph (spring bounce)
        val spinTarget = inst.rotation + pair.spinDelta * pair.gen
        val spin by animateFloatAsState(
            targetValue = spinTarget,
            animationSpec = spring(
                dampingRatio = Spring.DampingRatioMediumBouncy,
                stiffness = Spring.StiffnessLow,
            ),
            label = "spin-$i",
        )

        val morph = remember(pair.from, pair.to) { Morph(pair.from, pair.to) }
        return Triple(morph, t, spin)
    }

    val animated = placements.mapIndexed { i, inst ->
        val (morph, progress, spin) = rememberAnimatedShape(i, inst)
        Triple(inst, Triple(morph, progress, spin), Unit)
    }

    Canvas(modifier = modifier.fillMaxSize()) {
        animated.forEach { (inst, anim, _) ->
            val (morph, progress, spin) = anim
            val path = morph.toPath(progress = progress)
            val s = size.minDimension * inst.sizeFrac
            if (s < 1f) return@forEach
            val bounds = path.getBounds()
            val pathSize = maxOf(bounds.width, bounds.height).takeIf { it > 0f } ?: 1f
            val cx = bounds.left + bounds.width / 2f
            val cy = bounds.top + bounds.height / 2f
            val scale = s / pathSize
            val matrix = Matrix()
            matrix.translate(size.width * inst.xFrac, size.height * inst.yFrac)
            matrix.rotateZ(spin)
            matrix.scale(scale, scale)
            matrix.translate(-cx, -cy)
            path.transform(matrix)
            drawPath(path, color = color.copy(alpha = inst.alpha))
        }
    }
}

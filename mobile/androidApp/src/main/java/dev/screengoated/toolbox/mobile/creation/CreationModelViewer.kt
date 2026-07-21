package dev.screengoated.toolbox.mobile.creation

import android.net.Uri
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilledIconToggleButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import dev.romainguy.kotlin.math.Float3
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.i18n.Creation3dLocale
import io.github.sceneview.SceneView
import io.github.sceneview.SurfaceType
import io.github.sceneview.rememberEngine
import io.github.sceneview.rememberModelInstance
import io.github.sceneview.rememberModelLoader
import io.github.sceneview.node.ModelNode
import java.io.File

@Composable
internal fun CreationModelViewer(
    outputPath: String,
    viewModel: CreationNativeViewModel,
    strings: Creation3dLocale,
    accent: Color,
    modifier: Modifier = Modifier,
) {
    val modelFile by produceState<File?>(null, outputPath) {
        value = runCatching { viewModel.previewFile(outputPath, "glb") }.getOrNull()
    }
    var showGrid by remember(outputPath) { mutableStateOf(true) }
    var autoRotate by remember(outputPath) { mutableStateOf(false) }
    val currentAutoRotate by rememberUpdatedState(autoRotate)
    val runtimeNode = remember(outputPath) { arrayOfNulls<ModelNode>(1) }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.surfaceContainerLowest),
        contentAlignment = Alignment.Center,
    ) {
        val file = modelFile
        if (file == null) {
            CircularProgressIndicator()
        } else {
            if (showGrid) ModelGrid(accent)
            val engine = rememberEngine()
            val modelLoader = rememberModelLoader(engine)
            SceneView(
                modifier = Modifier.fillMaxSize(),
                surfaceType = SurfaceType.TextureSurface,
                engine = engine,
                modelLoader = modelLoader,
                isOpaque = false,
                autoFitContent = true,
                onFrame = { frameTimeNanos ->
                    if (currentAutoRotate) {
                        runtimeNode[0]?.rotation = Float3(
                            0f,
                            (frameTimeNanos / 1_000_000_000.0 * 18.0 % 360.0).toFloat(),
                            0f,
                        )
                    }
                },
            ) {
                rememberModelInstance(modelLoader, Uri.fromFile(file).toString())?.let { instance ->
                    ModelNode(
                        modelInstance = instance,
                        scaleToUnits = 1.0f,
                        autoAnimate = true,
                        apply = { runtimeNode[0] = this },
                    )
                }
            }
            Row(
                modifier = Modifier.align(Alignment.TopEnd).padding(10.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                ViewerToggle(
                    checked = showGrid,
                    icon = R.drawable.ms_grid_view,
                    label = strings.grid,
                    onCheckedChange = { showGrid = it },
                )
                ViewerToggle(
                    checked = autoRotate,
                    icon = R.drawable.ms_refresh,
                    label = strings.autoRotate,
                    onCheckedChange = { autoRotate = it },
                )
            }
        }
    }
}

@Composable
private fun ViewerToggle(
    checked: Boolean,
    icon: Int,
    label: String,
    onCheckedChange: (Boolean) -> Unit,
) {
    FilledIconToggleButton(
        checked = checked,
        onCheckedChange = onCheckedChange,
        modifier = Modifier.size(42.dp),
    ) {
        Icon(painterResource(icon), contentDescription = label, modifier = Modifier.size(19.dp))
    }
}

@Composable
private fun ModelGrid(accent: Color) {
    val line = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.45f)
    Canvas(Modifier.fillMaxSize()) {
        val step = size.minDimension / 11f
        var x = size.width / 2f % step
        while (x < size.width) {
            drawLine(line, androidx.compose.ui.geometry.Offset(x, 0f), androidx.compose.ui.geometry.Offset(x, size.height), 1f)
            x += step
        }
        var y = size.height / 2f % step
        while (y < size.height) {
            drawLine(line, androidx.compose.ui.geometry.Offset(0f, y), androidx.compose.ui.geometry.Offset(size.width, y), 1f)
            y += step
        }
        drawCircle(
            color = accent.copy(alpha = 0.35f),
            radius = step * 0.12f,
            center = center,
            style = Stroke(width = 1.5f),
        )
    }
}

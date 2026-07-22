package dev.screengoated.toolbox.mobile.creation

import android.view.MotionEvent
import android.view.View
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilledIconToggleButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
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
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.zIndex
import dev.romainguy.kotlin.math.Float3
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.i18n.Creation3dLocale
import io.github.sceneview.SceneView
import io.github.sceneview.SurfaceType
import io.github.sceneview.gesture.CameraGestureDetector
import io.github.sceneview.model.model
import io.github.sceneview.rememberEngine
import io.github.sceneview.rememberModelLoader
import io.github.sceneview.node.ModelNode
import java.io.File

private enum class MobileModelShading { ORIGINAL, TOON, PARTS }

@Composable
internal fun CreationModelViewer(
    outputPath: String,
    viewModel: CreationNativeViewModel,
    strings: Creation3dLocale,
    accent: Color,
    modifier: Modifier = Modifier,
) {
    val modelFile by produceState<Result<File>?>(null, outputPath) {
        value = runCatching { viewModel.previewFile(outputPath, "glb") }
    }
    var showGrid by remember(outputPath) { mutableStateOf(true) }
    var autoRotate by remember(outputPath) { mutableStateOf(false) }
    var shading by remember(outputPath) { mutableStateOf(MobileModelShading.ORIGINAL) }
    var fitRevision by remember(outputPath) { mutableStateOf(0) }
    val currentAutoRotate by rememberUpdatedState(autoRotate)
    val runtimeNode = remember(outputPath) { arrayOfNulls<ModelNode>(1) }
    var materialController by remember(outputPath) { mutableStateOf<ModelMaterialController?>(null) }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.surfaceContainerLowest),
        contentAlignment = Alignment.Center,
    ) {
        val fileResult = modelFile
        if (fileResult == null) {
            CircularProgressIndicator()
        } else if (fileResult.isFailure) {
            Text(strings.previewUnavailable, color = MaterialTheme.colorScheme.error)
        } else {
            val file = fileResult.getOrThrow()
            if (showGrid) ModelGrid(accent)
            val engine = rememberEngine()
            val modelLoader = rememberModelLoader(engine)
            val modelInstance = remember(file, modelLoader) {
                runCatching { modelLoader.createModelInstance(file) }
            }
            val instance = modelInstance.getOrNull()
            DisposableEffect(instance, modelLoader) {
                onDispose {
                    instance?.let { modelLoader.destroyModel(it.model) }
                }
            }
            val currentMaterialController = materialController
            DisposableEffect(currentMaterialController) {
                onDispose { currentMaterialController?.destroy() }
            }
            LaunchedEffect(shading, materialController) {
                materialController?.apply(shading)
            }
            if (instance == null) {
                Text(strings.previewUnavailable, color = MaterialTheme.colorScheme.error)
            } else {
                val cameraManipulator = remember(fitRevision) {
                    CameraGestureDetector.DefaultCameraManipulator(
                        Float3(0f, 0f, 1.8f),
                        Float3(0f, 0f, 0f),
                    )
                }
                val gestureView = remember { arrayOfNulls<View>(1) }
                val gestureDetector = remember {
                    CameraGestureDetector(
                        viewHeight = { gestureView[0]?.height?.coerceAtLeast(1) ?: 1 },
                        cameraManipulator = cameraManipulator,
                    )
                }
                SideEffect { gestureDetector.cameraManipulator = cameraManipulator }
                SceneView(
                    modifier = Modifier.fillMaxSize(),
                    surfaceType = SurfaceType.Surface,
                    engine = engine,
                    modelLoader = modelLoader,
                    isOpaque = false,
                    cameraManipulator = cameraManipulator,
                    onFrame = { frameTimeNanos ->
                        val node = runtimeNode[0]
                        if (currentAutoRotate) {
                            node?.rotation = Float3(
                                0f,
                                (frameTimeNanos / 1_000_000_000.0 * 18.0 % 360.0).toFloat(),
                                0f,
                            )
                        }
                    },
                ) {
                    ModelNode(
                        modelInstance = instance,
                        scaleToUnits = 1.0f,
                        centerOrigin = Float3(0f, 0f, 0f),
                        autoAnimate = true,
                        apply = {
                            runtimeNode[0] = this
                            materialController = ModelMaterialController(this, engine).also {
                                it.apply(shading)
                            }
                        },
                    )
                }
                ModelGestureLayer(
                    detector = gestureDetector,
                    onView = { gestureView[0] = it },
                    modifier = Modifier.fillMaxSize().padding(top = 58.dp).zIndex(1f),
                )
            }
            FlowRow(
                modifier = Modifier.align(Alignment.TopEnd).padding(10.dp).zIndex(2f),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                ViewerToggle(
                    checked = shading == MobileModelShading.ORIGINAL,
                    icon = R.drawable.ms_image,
                    label = strings.originalMaterials,
                    onCheckedChange = { if (it) shading = MobileModelShading.ORIGINAL },
                )
                ViewerToggle(
                    checked = shading == MobileModelShading.TOON,
                    icon = R.drawable.ms_auto_awesome,
                    label = strings.toonOutline,
                    onCheckedChange = { if (it) shading = MobileModelShading.TOON },
                )
                ViewerToggle(
                    checked = shading == MobileModelShading.PARTS,
                    icon = R.drawable.ms_layers,
                    label = strings.partColors,
                    onCheckedChange = { if (it) shading = MobileModelShading.PARTS },
                )
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
                ViewerToggle(
                    checked = false,
                    icon = R.drawable.ms_open_in_full,
                    label = strings.fit,
                    onCheckedChange = { fitRevision += 1 },
                )
            }
        }
    }
}

@Composable
private fun ModelGestureLayer(
    detector: CameraGestureDetector,
    onView: (View) -> Unit,
    modifier: Modifier = Modifier,
) {
    // Own the complete MotionEvent stream so a surrounding LazyColumn cannot cancel orbit or pinch.
    AndroidView(
        modifier = modifier,
        factory = { context ->
            View(context).apply {
                isClickable = true
                isFocusable = true
            }
        },
        update = { view ->
            onView(view)
            view.setOnTouchListener { touchedView, event ->
                if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                    touchedView.parent?.requestDisallowInterceptTouchEvent(true)
                }
                detector.onTouchEvent(event)
                if (event.actionMasked == MotionEvent.ACTION_UP ||
                    event.actionMasked == MotionEvent.ACTION_CANCEL
                ) {
                    touchedView.parent?.requestDisallowInterceptTouchEvent(false)
                }
                true
            }
        },
    )
}

private class ModelMaterialController(
    private val node: ModelNode,
    private val engine: com.google.android.filament.Engine,
) {
    private val original = node.materialInstances.map { it.toList() }
    private val palette = listOf(
        floatArrayOf(0.10f, 0.67f, 0.57f),
        floatArrayOf(0.24f, 0.45f, 0.86f),
        floatArrayOf(0.94f, 0.48f, 0.31f),
        floatArrayOf(0.91f, 0.69f, 0.25f),
        floatArrayOf(0.54f, 0.37f, 0.78f),
        floatArrayOf(0.20f, 0.70f, 0.78f),
    )
    private val toon = duplicateMaterials(MobileModelShading.TOON)
    private val parts = duplicateMaterials(MobileModelShading.PARTS)
    private var destroyed = false

    fun apply(mode: MobileModelShading) {
        if (destroyed) return
        node.materialInstances = when (mode) {
            MobileModelShading.ORIGINAL -> original
            MobileModelShading.TOON -> toon
            MobileModelShading.PARTS -> parts
        }
    }

    fun destroy() {
        if (destroyed) return
        destroyed = true
        (toon + parts).flatten().forEach { material ->
            runCatching { engine.destroyMaterialInstance(material) }
        }
    }

    private fun duplicateMaterials(mode: MobileModelShading) = original.map { group ->
        group.mapIndexed { index, source ->
            com.google.android.filament.MaterialInstance.duplicate(
                source,
                "sgt_${mode.name.lowercase()}_${source.name}_$index",
            ).apply {
                runCatching { setParameter("metallicFactor", 0.0f) }
                runCatching { setParameter("roughnessFactor", 0.88f) }
                if (mode == MobileModelShading.PARTS) {
                    val color = palette[index % palette.size]
                    runCatching {
                        setParameter(
                            "baseColorFactor",
                            com.google.android.filament.Colors.RgbaType.SRGB,
                            color[0], color[1], color[2], 1f,
                        )
                    }
                }
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

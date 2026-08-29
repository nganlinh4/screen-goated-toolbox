package dev.screengoated.toolbox.mobile.creation

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.ui.UtilityStatusChip
import dev.screengoated.toolbox.mobile.ui.i18n.CreationCommonLocale
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.launch
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.jsonPrimitive

@Composable
internal fun CreationActiveSettings(
    tool: CreationTool,
    state: CreationNativeUiState,
    locale: MobileLocaleText,
    accent: Color,
    viewModel: CreationNativeViewModel,
    onPickImages: () -> Unit,
) {
    val item = state.selectedItem
    if (state.tab != CreationNativeTab.JOBS || item == null) return
    val enabled = if (tool == CreationTool.IMAGE_CREATOR) {
        !item.submitted && item.stage == CreationNativeStage.DRAFT
    } else {
        item.isConfigurable()
    }
    when (tool) {
        CreationTool.IMAGE_TO_3D -> if (item.status?.outputPath != null) {
            Creation3dRefinementPanel(
                status = item.status,
                strings = locale.creationApps.model3d.refinement,
                accent = accent,
                onRefine = viewModel::refineSelected,
            )
        } else {
            Creation3dSettings(
                item = item,
                strings = locale.creationApps.model3d,
                accent = accent,
                enabled = enabled,
                onPolycount = viewModel::setPolycount,
                onAutoSegment = viewModel::setAutoSegment,
                onInstruction = viewModel::setInstruction,
            )
        }
        CreationTool.IMAGE_TO_SVG -> CreationSvgSettings(
            item = item,
            strings = locale.creationApps.svg,
            accent = accent,
            enabled = enabled,
            onModel = viewModel::setModel,
            onBackgroundMode = viewModel::setSvgBackgroundMode,
        )
        CreationTool.IMAGE_CREATOR -> CreationImageSettings(
            item = item,
            strings = locale.creationApps.image,
            accent = accent,
            enabled = enabled,
            onPrompt = viewModel::setPrompt,
            onAddReferences = onPickImages,
            onRemoveReference = viewModel::removeImageReference,
        )
    }
}

@Composable
internal fun CreationActiveWorkbench(
    tool: CreationTool,
    state: CreationNativeUiState,
    locale: MobileLocaleText,
    accent: Color,
    viewModel: CreationNativeViewModel,
    onPickImages: () -> Unit,
    fillAvailable: Boolean = false,
) {
    val item = state.selectedItem
    val history = state.selectedHistory
    val outputPath = if (state.tab == CreationNativeTab.RESULTS) {
        history?.outputPath
    } else {
        item?.status?.outputPath
    }
    val outputName = if (state.tab == CreationNativeTab.RESULTS) {
        history?.outputName
    } else {
        item?.status?.outputName
    }
    val outputSegmented = if (state.tab == CreationNativeTab.RESULTS) {
        history?.metadata?.get("isSegmented")?.jsonPrimitive?.booleanOrNull
    } else {
        item?.status?.isSegmented
    } ?: false
    val referencePaths = if (tool == CreationTool.IMAGE_CREATOR) {
        if (state.tab == CreationNativeTab.RESULTS && history != null) {
            CreationImageSessions.historyReferences(history)
        } else {
            item?.referencePaths.orEmpty()
        }
    } else {
        emptyList()
    }
    val controller = remember(outputPath) { CreationSvgDocumentController() }
    var svgEditingRequested by remember(outputPath) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    Column(
        modifier = if (fillAvailable) Modifier.fillMaxSize() else Modifier,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        CreationWorkbench(
            modifier = if (fillAvailable) Modifier.weight(1f) else Modifier,
            accent = accent,
            fillAvailable = fillAvailable,
        ) {
            when {
                outputPath != null && tool == CreationTool.IMAGE_TO_3D -> {
                    Box(Modifier.fillMaxSize()) {
                        CreationModelViewer(
                            outputPath = outputPath,
                            segmented = outputSegmented,
                            viewModel = viewModel,
                            strings = locale.creationApps.model3d,
                        )
                        if (item?.stage in setOf(
                                CreationNativeStage.QUEUED,
                                CreationNativeStage.RUNNING,
                            )
                        ) {
                            CreationProgressOverlay(
                                status = item?.status,
                                common = locale.creationApps.common,
                                accent = accent,
                            )
                        }
                    }
                }
                outputPath != null && tool == CreationTool.IMAGE_TO_SVG -> {
                    CreationSvgDocument(
                        outputPath = outputPath,
                        viewModel = viewModel,
                        controller = controller,
                        editingRequested = svgEditingRequested,
                    )
                }
                outputPath != null && tool == CreationTool.IMAGE_CREATOR -> {
                    CreationImageResult(
                        referencePaths = referencePaths,
                        outputPath = outputPath,
                        viewModel = viewModel,
                        strings = locale.creationApps.image,
                    )
                }
                item != null && tool == CreationTool.IMAGE_CREATOR -> {
                    Box(Modifier.fillMaxSize()) {
                        CreationImageSource(
                            referencePaths = item.referencePaths,
                            strings = locale.creationApps.image,
                            accent = accent,
                        )
                        if (item.stage in setOf(
                                CreationNativeStage.QUEUED,
                                CreationNativeStage.RUNNING,
                            )
                        ) {
                            CreationProgressOverlay(
                                status = item.status,
                                common = locale.creationApps.common,
                                accent = accent,
                            )
                        }
                    }
                }
                item != null -> {
                    CreationSourceWorkbench(item)
                    if (item.stage in setOf(
                            CreationNativeStage.QUEUED,
                            CreationNativeStage.RUNNING,
                        )
                    ) {
                        CreationProgressOverlay(
                            status = item.status,
                            common = locale.creationApps.common,
                            accent = accent,
                        )
                    }
                }
                else -> CreationEmptyWorkbench(locale.creationApps.common, accent, onPickImages)
            }
        }
        item?.status?.error
            ?.takeIf { item.stage == CreationNativeStage.FAILED }
            ?.let { message ->
                Text(
                    text = publicCreationErrorText(message, locale.creationApps.common),
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        if (outputPath != null &&
            tool == CreationTool.IMAGE_TO_SVG &&
            !svgEditingRequested
        ) {
            FilledTonalButton(
                onClick = { svgEditingRequested = true },
                modifier = Modifier.fillMaxWidth(),
            ) {
                Icon(painterResource(dev.screengoated.toolbox.mobile.R.drawable.ms_edit), null)
                Text(locale.creationApps.svg.editPaths)
            }
        }
        if (outputPath != null &&
            tool == CreationTool.IMAGE_TO_SVG &&
            controller.isEditable
        ) {
            CreationSvgEditorControls(
                controller = controller,
                common = locale.creationApps.common,
                strings = locale.creationApps.svg,
                accent = accent,
                onSave = {
                    scope.launch {
                        val updated = controller.serialize()
                        if (updated.isNotBlank()) viewModel.saveSvg(outputPath, updated)
                    }
                },
            )
        }
        if (outputPath != null) {
            CreationResultSummary(
                tool = tool,
                state = state,
                name = outputName.orEmpty(),
                accent = accent,
                locale = locale,
                viewModel = viewModel,
            )
        }
    }
}

@Composable
private fun CreationResultSummary(
    tool: CreationTool,
    state: CreationNativeUiState,
    name: String,
    accent: Color,
    locale: MobileLocaleText,
    viewModel: CreationNativeViewModel,
) {
    val item = state.selectedItem
    val history = state.selectedHistory
    val common = locale.creationApps.common
    var rename by remember(history?.id) { mutableStateOf(false) }
    val faces = item?.status?.faces ?: history.longMetadata("faces")
    val vertices = item?.status?.vertices ?: history.longMetadata("vertices")
    val polygons = item?.status?.polygons ?: history.longMetadata("polygons")
    val quads = item?.status?.quads ?: history.longMetadata("quads")
    val downloadPath = item?.status?.downloadPath ?: history.downloadMetadata("path")
    val downloadName = item?.status?.downloadName ?: history.downloadMetadata("name")
    val width = item?.status?.width ?: history.intMetadata("width")
    val height = item?.status?.height ?: history.intMetadata("height")
    val segmented = item?.status?.isSegmented
        ?: history?.metadata?.get("isSegmented")?.jsonPrimitive?.booleanOrNull
        ?: false
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    listOfNotNull(name.takeIf(String::isNotBlank), downloadName)
                        .distinct()
                        .joinToString(" · "),
                    style = MaterialTheme.typography.titleSmall,
                    maxLines = 1,
                )
                when {
                    tool == CreationTool.IMAGE_TO_3D &&
                        polygons != null && quads != null -> {
                        Text(
                            quadGeometryStatsText(
                                locale.creationApps.model3d.quadGeometryStats,
                                vertices,
                                polygons,
                                quads,
                            ),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    tool == CreationTool.IMAGE_TO_3D && (faces != null || vertices != null) -> {
                        Text(
                            geometryStatsText(
                                locale.creationApps.model3d.geometryStats,
                                vertices,
                                faces,
                            ),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    tool == CreationTool.IMAGE_CREATOR && width != null && height != null -> {
                        Text(
                            "$width × $height px",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
            UtilityStatusChip(
                text = when (tool) {
                    CreationTool.IMAGE_TO_3D -> {
                        if (segmented) locale.creationApps.model3d.partsReady
                        else locale.creationApps.model3d.modelReady
                    }
                    CreationTool.IMAGE_TO_SVG -> locale.creationApps.svg.vectorReady
                    CreationTool.IMAGE_CREATOR -> locale.creationApps.image.imageReady
                },
                accent = accent,
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val path = history?.outputPath ?: item?.status?.outputPath
            if (tool == CreationTool.IMAGE_TO_3D) {
                TextButton(
                    onClick = viewModel::exportSelectedRevision,
                    modifier = Modifier.testTag("creation-download-revision"),
                ) {
                    Icon(painterResource(dev.screengoated.toolbox.mobile.R.drawable.ms_download), null)
                    Text(locale.creationApps.model3d.download)
                }
            } else {
                TextButton(onClick = { path?.let(viewModel::openOutput) }) { Text(common.open) }
                if (downloadPath != null) {
                    TextButton(onClick = { viewModel.openOutput(downloadPath) }) { Text("FBX") }
                }
            }
            if (history != null) {
                TextButton(onClick = { rename = true }) { Text(common.rename) }
                TextButton(onClick = { viewModel.deleteHistory(history.id) }) {
                    Text(common.delete)
                }
            }
        }
    }
    if (rename && history != null) {
        RenameResultDialog(
            initialName = history.outputName,
            common = common,
            onDismiss = { rename = false },
            onRename = {
                viewModel.renameHistory(history.id, it)
                rename = false
            },
        )
    }
}

private fun CreationHistoryEntry?.longMetadata(key: String): Long? =
    this?.metadata?.get(key)?.jsonPrimitive?.longOrNull

private fun CreationHistoryEntry?.intMetadata(key: String): Int? =
    this?.metadata?.get(key)?.jsonPrimitive?.intOrNull

private fun CreationHistoryEntry?.downloadMetadata(key: String): String? =
    this?.metadata?.get("download")?.jsonObject?.get(key)?.jsonPrimitive?.contentOrNull

private fun geometryStatsText(template: String, vertices: Long?, faces: Long?): String =
    template.replaceFirst("{}", vertices?.toString() ?: "-")
        .replaceFirst("{}", faces?.toString() ?: "-")

private fun quadGeometryStatsText(
    template: String,
    vertices: Long?,
    polygons: Long,
    quads: Long,
): String = template.replaceFirst("{}", vertices?.toString() ?: "-")
    .replaceFirst("{}", polygons.toString())
    .replaceFirst("{}", quads.toString())

@Composable
private fun RenameResultDialog(
    initialName: String,
    common: CreationCommonLocale,
    onDismiss: () -> Unit,
    onRename: (String) -> Unit,
) {
    var value by remember(initialName) { mutableStateOf(initialName) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(common.rename) },
        text = {
            OutlinedTextField(
                value = value,
                onValueChange = { value = it },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
        },
        confirmButton = {
            TextButton(onClick = { onRename(value) }, enabled = value.isNotBlank()) {
                Text(common.rename)
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(common.dismiss) } },
    )
}

@file:OptIn(
    androidx.compose.material3.ExperimentalMaterial3Api::class,
    androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class,
)

package dev.screengoated.toolbox.mobile.creation

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRowDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.UtilityStatusChip
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.launch
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.jsonPrimitive

private val ModelAccent = Color(0xff008f7a)
private val VectorAccent = Color(0xff3568d4)

@Composable
internal fun CreationNativeScreen(
    tool: CreationTool,
    state: CreationNativeUiState,
    locale: MobileLocaleText,
    viewModel: CreationNativeViewModel,
    onBack: () -> Unit,
    onPickImages: () -> Unit,
    onPickOutputDirectory: () -> Unit,
) {
    val common = locale.creationApps.common
    val accent = if (tool == CreationTool.IMAGE_TO_3D) ModelAccent else VectorAccent
    val title = if (tool == CreationTool.IMAGE_TO_3D) {
        locale.creationApps.appImageTo3dTitle
    } else {
        locale.creationApps.appImageToSvgTitle
    }
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(state.transientError) {
        state.transientError?.let {
            snackbar.showSnackbar(it)
            viewModel.dismissError()
        }
    }

    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ms_arrow_back), contentDescription = null)
                    }
                },
                actions = {
                    CreationReadinessChip(state.preparationStatus, common, accent)
                    Spacer(Modifier.width(8.dp))
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surface,
                ),
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
        bottomBar = {
            CreationBottomActions(
                tool = tool,
                state = state,
                locale = locale,
                accent = accent,
                viewModel = viewModel,
            )
        },
    ) { padding ->
        BoxWithConstraints(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            val wide = maxWidth >= 840.dp
            Column(modifier = Modifier.fillMaxSize()) {
                if (wide) {
                    CreationWideBody(
                        tool = tool,
                        state = state,
                        locale = locale,
                        accent = accent,
                        viewModel = viewModel,
                        onPickImages = onPickImages,
                        onPickOutputDirectory = onPickOutputDirectory,
                    )
                } else {
                    CreationTabs(state, common.jobs, common.results, accent, viewModel::showTab)
                    CreationItemRail(
                        state = state,
                        locale = locale,
                        accent = accent,
                        viewModel = viewModel,
                        onPickImages = onPickImages,
                    )
                    HorizontalDivider()
                    Box(modifier = Modifier.weight(1f)) {
                        CreationPhoneBody(
                            tool = tool,
                            state = state,
                            locale = locale,
                            accent = accent,
                            viewModel = viewModel,
                            onPickImages = onPickImages,
                            onPickOutputDirectory = onPickOutputDirectory,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun CreationTabs(
    state: CreationNativeUiState,
    jobs: String,
    results: String,
    accent: Color,
    onTab: (CreationNativeTab) -> Unit,
    compact: Boolean = false,
) {
    PrimaryTabRow(
        selectedTabIndex = state.tab.ordinal,
        containerColor = MaterialTheme.colorScheme.surface,
        contentColor = accent,
        indicator = {
            TabRowDefaults.PrimaryIndicator(
                modifier = Modifier.tabIndicatorOffset(state.tab.ordinal),
                color = accent,
            )
        },
    ) {
        Tab(
            selected = state.tab == CreationNativeTab.JOBS,
            onClick = { onTab(CreationNativeTab.JOBS) },
            text = { Text("$jobs (${state.items.size})") },
            icon = if (compact) null else {
                { Icon(painterResource(R.drawable.ms_tune), contentDescription = null) }
            },
        )
        Tab(
            selected = state.tab == CreationNativeTab.RESULTS,
            onClick = { onTab(CreationNativeTab.RESULTS) },
            text = { Text("$results (${state.history.size})") },
            icon = if (compact) null else {
                { Icon(painterResource(R.drawable.ms_history), contentDescription = null) }
            },
        )
    }
}

@Composable
private fun CreationItemRail(
    state: CreationNativeUiState,
    locale: MobileLocaleText,
    accent: Color,
    viewModel: CreationNativeViewModel,
    onPickImages: () -> Unit,
    compact: Boolean = false,
) {
    val horizontalPadding = if (compact) 0.dp else 16.dp
    val verticalPadding = if (compact) 6.dp else 12.dp
    Column(Modifier.padding(horizontal = horizontalPadding, vertical = verticalPadding)) {
        if (state.tab == CreationNativeTab.JOBS) {
            CreationQueueStrip(
                items = state.items,
                selectedId = state.selectedItemId,
                common = locale.creationApps.common,
                accent = accent,
                onSelect = viewModel::selectItem,
                onRemove = viewModel::removeDraft,
                onAdd = onPickImages,
            )
        } else {
            CreationHistoryStrip(
                entries = state.history,
                selectedId = state.selectedHistoryId,
                common = locale.creationApps.common,
                accent = accent,
                onSelect = viewModel::selectHistory,
            )
        }
    }
}

@Composable
private fun CreationPhoneBody(
    tool: CreationTool,
    state: CreationNativeUiState,
    locale: MobileLocaleText,
    accent: Color,
    viewModel: CreationNativeViewModel,
    onPickImages: () -> Unit,
    onPickOutputDirectory: () -> Unit,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        item {
            CreationActiveWorkbench(tool, state, locale, accent, viewModel, onPickImages)
        }
        item { CreationActiveSettings(tool, state, locale, accent, viewModel) }
        item {
            CreationOutputSettings(
                outputDirectory = state.outputDirectory,
                common = locale.creationApps.common,
                accent = accent,
                onChangeFolder = onPickOutputDirectory,
            )
        }
    }
}

@Composable
private fun CreationWideBody(
    tool: CreationTool,
    state: CreationNativeUiState,
    locale: MobileLocaleText,
    accent: Color,
    viewModel: CreationNativeViewModel,
    onPickImages: () -> Unit,
    onPickOutputDirectory: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 20.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        Column(modifier = Modifier.weight(0.38f).fillMaxSize()) {
            CreationTabs(
                state,
                locale.creationApps.common.jobs,
                locale.creationApps.common.results,
                accent,
                viewModel::showTab,
                compact = true,
            )
            CreationItemRail(
                state,
                locale,
                accent,
                viewModel,
                onPickImages,
                compact = true,
            )
            HorizontalDivider()
            LazyColumn(
                modifier = Modifier.weight(1f),
                contentPadding = androidx.compose.foundation.layout.PaddingValues(
                    top = 8.dp,
                    bottom = 8.dp,
                ),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                item { CreationActiveSettings(tool, state, locale, accent, viewModel) }
                item {
                    CreationOutputSettings(
                        outputDirectory = state.outputDirectory,
                        common = locale.creationApps.common,
                        accent = accent,
                        onChangeFolder = onPickOutputDirectory,
                    )
                }
            }
        }
        Column(modifier = Modifier.weight(0.62f).fillMaxSize()) {
            CreationActiveWorkbench(
                tool,
                state,
                locale,
                accent,
                viewModel,
                onPickImages,
                fillAvailable = true,
            )
        }
    }
}

@Composable
private fun CreationActiveSettings(
    tool: CreationTool,
    state: CreationNativeUiState,
    locale: MobileLocaleText,
    accent: Color,
    viewModel: CreationNativeViewModel,
) {
    val item = state.selectedItem
    if (state.tab != CreationNativeTab.JOBS || item == null) return
    val enabled = item.stage == CreationNativeStage.DRAFT && !item.submitted
    if (tool == CreationTool.IMAGE_TO_3D) {
        Creation3dSettings(
            item = item,
            strings = locale.creationApps.model3d,
            accent = accent,
            enabled = enabled,
            onPolycount = viewModel::setPolycount,
            onAutoSegment = viewModel::setAutoSegment,
        )
    } else {
        CreationSvgSettings(
            item = item,
            strings = locale.creationApps.svg,
            accent = accent,
            enabled = enabled,
            onModel = viewModel::setModel,
        )
    }
}

@Composable
private fun CreationActiveWorkbench(
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
    val controller = remember(outputPath) { CreationSvgDocumentController() }
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
                            viewModel = viewModel,
                            strings = locale.creationApps.model3d,
                            accent = accent,
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
                                hasDepthPreview = true,
                            )
                        }
                    }
                }
                outputPath != null -> {
                    CreationSvgDocument(outputPath, viewModel, controller)
                }
                item != null -> {
                    CreationSourceWorkbench(tool, item)
                    if (item.stage in setOf(CreationNativeStage.QUEUED, CreationNativeStage.RUNNING)) {
                        CreationProgressOverlay(
                            status = item.status,
                            common = locale.creationApps.common,
                            accent = accent,
                            hasDepthPreview = item.depthPreviewPath != null,
                        )
                    }
                }
                else -> CreationEmptyWorkbench(locale.creationApps.common, accent, onPickImages)
            }
        }
        if (outputPath != null && tool == CreationTool.IMAGE_TO_SVG) {
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
    var delete by remember(history?.id) { mutableStateOf(false) }
    val faces = item?.status?.faces ?: history?.metadata?.get("faces")?.jsonPrimitive?.longOrNull
    val vertices = item?.status?.vertices ?: history?.metadata?.get("vertices")?.jsonPrimitive?.longOrNull
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
                Text(name, style = MaterialTheme.typography.titleSmall, maxLines = 1)
                if (tool == CreationTool.IMAGE_TO_3D && (faces != null || vertices != null)) {
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
            }
            UtilityStatusChip(
                text = if (tool == CreationTool.IMAGE_TO_3D) {
                    if (segmented) locale.creationApps.model3d.partsReady
                    else locale.creationApps.model3d.modelReady
                } else locale.creationApps.svg.vectorReady,
                accent = accent,
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val path = history?.outputPath ?: item?.status?.outputPath
            TextButton(onClick = { path?.let(viewModel::openOutput) }) { Text(common.open) }
            if (history != null) {
                TextButton(onClick = { rename = true }) { Text(common.rename) }
                TextButton(onClick = { delete = true }) { Text(common.delete) }
            }
        }
    }
    if (rename && history != null) {
        RenameResultDialog(
            initialName = history.outputName,
            common = common,
            onDismiss = { rename = false },
            onRename = { viewModel.renameHistory(history.id, it); rename = false },
        )
    }
    if (delete && history != null) {
        AlertDialog(
            onDismissRequest = { delete = false },
            title = { Text(common.delete) },
            text = { Text(common.deleteConfirm) },
            confirmButton = {
                TextButton(onClick = { viewModel.deleteHistory(history.id); delete = false }) {
                    Text(common.delete)
                }
            },
            dismissButton = { TextButton(onClick = { delete = false }) { Text(common.dismiss) } },
        )
    }
}

private fun geometryStatsText(template: String, vertices: Long?, faces: Long?): String =
    template.replaceFirst("{}", vertices?.toString() ?: "-")
        .replaceFirst("{}", faces?.toString() ?: "-")

@Composable
private fun RenameResultDialog(
    initialName: String,
    common: dev.screengoated.toolbox.mobile.ui.i18n.CreationCommonLocale,
    onDismiss: () -> Unit,
    onRename: (String) -> Unit,
) {
    var value by remember(initialName) { mutableStateOf(initialName) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(common.rename) },
        text = {
            androidx.compose.material3.OutlinedTextField(
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

@Composable
private fun CreationBottomActions(
    tool: CreationTool,
    state: CreationNativeUiState,
    locale: MobileLocaleText,
    accent: Color,
    viewModel: CreationNativeViewModel,
) {
    if (state.tab != CreationNativeTab.JOBS || state.selectedItem == null) return
    val item = requireNotNull(state.selectedItem)
    val common = locale.creationApps.common
    val label = when {
        item.stage == CreationNativeStage.FAILED || item.stage == CreationNativeStage.CANCELLED ->
            common.retry
        item.stage == CreationNativeStage.RUNNING -> common.cancel
        item.stage == CreationNativeStage.DONE && tool == CreationTool.IMAGE_TO_3D &&
            item.status?.canSegment == true && !item.status.isSegmented ->
            locale.creationApps.model3d.separate
        item.stage == CreationNativeStage.DONE -> return
        else -> if (tool == CreationTool.IMAGE_TO_3D) locale.creationApps.model3d.generate
        else locale.creationApps.svg.generate
    }
    val action = when {
        item.stage == CreationNativeStage.RUNNING -> viewModel::cancelSelected
        item.stage == CreationNativeStage.DONE -> viewModel::segmentSelected
        else -> viewModel::submitSelected
    }
    androidx.compose.material3.Surface(tonalElevation = 3.dp) {
        Button(
            onClick = action,
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .padding(horizontal = 16.dp, vertical = 10.dp)
                .height(52.dp),
            enabled = item.stage != CreationNativeStage.QUEUED,
            colors = androidx.compose.material3.ButtonDefaults.buttonColors(containerColor = accent),
            shape = MaterialTheme.shapes.medium,
        ) {
            Icon(
                painterResource(
                    if (item.stage == CreationNativeStage.RUNNING) R.drawable.ms_close
                    else R.drawable.ms_auto_awesome,
                ),
                contentDescription = null,
            )
            Spacer(Modifier.width(8.dp))
            Text(label)
        }
    }
}

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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.AlertDialog
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
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

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
    val accent = when (tool) {
        CreationTool.IMAGE_TO_3D -> ModelAccent
        CreationTool.IMAGE_TO_SVG -> VectorAccent
        CreationTool.IMAGE_CREATOR -> VectorAccent
    }
    val title = when (tool) {
        CreationTool.IMAGE_TO_3D -> locale.creationApps.appImageTo3dTitle
        CreationTool.IMAGE_TO_SVG -> locale.creationApps.appImageToSvgTitle
        CreationTool.IMAGE_CREATOR -> locale.creationApps.appImageCreatorTitle
    }
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(viewModel) {
        withFrameNanos { }
        viewModel.activateSurface()
    }
    LaunchedEffect(state.transientError) {
        state.transientError?.let {
            snackbar.showSnackbar(publicCreationErrorText(it, common))
            viewModel.dismissError()
        }
    }

    Scaffold(
        modifier = Modifier
            .testTag("creation-root")
            .semantics { testTagsAsResourceId = true },
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(
                            painterResource(R.drawable.ms_arrow_back),
                            contentDescription = common.dismiss,
                        )
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
                        tool = tool,
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
    tool: CreationTool,
    state: CreationNativeUiState,
    locale: MobileLocaleText,
    accent: Color,
    viewModel: CreationNativeViewModel,
    onPickImages: () -> Unit,
    compact: Boolean = false,
) {
    var confirmDeleteAll by remember(tool) { mutableStateOf(false) }
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
                onAdd = if (tool == CreationTool.IMAGE_CREATOR) {
                    viewModel::addImageSession
                } else {
                    onPickImages
                },
                addLabel = if (tool == CreationTool.IMAGE_CREATOR) {
                    locale.creationApps.image.newImage
                } else {
                    locale.creationApps.common.addImages
                },
                itemLabel = { item ->
                    when {
                        tool != CreationTool.IMAGE_CREATOR -> item.sourceName
                        item.referencePaths.isEmpty() -> locale.creationApps.image.newImage
                        item.referencePaths.size == 1 -> item.sourceName
                        else -> locale.creationApps.image.referenceCount
                            .replace("{}", item.referencePaths.size.toString())
                    }
                },
                showArtworkPreviews = tool == CreationTool.IMAGE_TO_3D,
            )
        } else {
            if (state.history.isNotEmpty()) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                ) {
                    TextButton(onClick = { confirmDeleteAll = true }) {
                        Icon(
                            painterResource(R.drawable.ms_delete),
                            contentDescription = null,
                        )
                        Spacer(Modifier.width(6.dp))
                        Text(locale.creationApps.common.deleteAll)
                    }
                }
            }
            CreationHistoryStrip(
                entries = state.history,
                selectedId = state.selectedHistoryId,
                common = locale.creationApps.common,
                accent = accent,
                onSelect = viewModel::selectHistory,
            )
        }
    }
    if (confirmDeleteAll) {
        val common = locale.creationApps.common
        AlertDialog(
            onDismissRequest = { confirmDeleteAll = false },
            title = { Text(common.deleteAll) },
            text = { Text(common.deleteAllConfirm) },
            confirmButton = {
                TextButton(
                    onClick = {
                        confirmDeleteAll = false
                        viewModel.deleteAllHistory()
                    },
                ) {
                    Text(common.deleteAll)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmDeleteAll = false }) {
                    Text(common.dismiss)
                }
            },
        )
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
        item {
            CreationActiveSettings(tool, state, locale, accent, viewModel, onPickImages)
        }
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
                tool,
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
                item {
                    CreationActiveSettings(tool, state, locale, accent, viewModel, onPickImages)
                }
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

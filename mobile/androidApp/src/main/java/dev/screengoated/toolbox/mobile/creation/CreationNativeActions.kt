package dev.screengoated.toolbox.mobile.creation

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun CreationBottomActions(
    tool: CreationTool,
    state: CreationNativeUiState,
    locale: MobileLocaleText,
    accent: Color,
    viewModel: CreationNativeViewModel,
) {
    if (state.tab != CreationNativeTab.JOBS || state.selectedItem == null) return
    val item = requireNotNull(state.selectedItem)
    val active = item.submitted && item.stage in setOf(CreationNativeStage.QUEUED, CreationNativeStage.RUNNING)
    if (tool == CreationTool.IMAGE_TO_3D && item.stage == CreationNativeStage.DONE) return
    val common = locale.creationApps.common
    val generate = when (tool) {
        CreationTool.IMAGE_TO_3D -> locale.creationApps.model3d.generate
        CreationTool.IMAGE_TO_SVG -> locale.creationApps.svg.generate
        CreationTool.IMAGE_CREATOR -> locale.creationApps.image.generate
    }
    val generateAgain = when (tool) {
        CreationTool.IMAGE_TO_3D -> locale.creationApps.model3d.generateAgain
        CreationTool.IMAGE_TO_SVG -> locale.creationApps.svg.generateAgain
        CreationTool.IMAGE_CREATOR -> locale.creationApps.image.generateAgain
    }
    val label = when {
        item.stage == CreationNativeStage.FAILED || item.stage == CreationNativeStage.CANCELLED ->
            common.retry
        active -> common.cancel
        item.stage == CreationNativeStage.DONE -> generateAgain
        else -> generate
    }
    val action = if (active) {
        viewModel::cancelSelected
    } else {
        viewModel::submitSelected
    }
    androidx.compose.material3.Surface(tonalElevation = 3.dp) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .padding(horizontal = 16.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            CreationActionButton(
                label = label,
                accent = accent,
                onClick = action,
                enabled = state.validatingItemId == null &&
                    (tool != CreationTool.IMAGE_CREATOR ||
                        item.prompt.isNotBlank() ||
                        active),
                cancel = active,
                modifier = Modifier
                    .weight(1f)
                    .testTag(if (active) "creation-cancel-action" else "creation-primary-action"),
            )
        }
    }
}

@Composable
private fun CreationActionButton(
    label: String,
    accent: Color,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    cancel: Boolean = false,
) {
    Button(
        onClick = onClick,
        modifier = modifier.height(52.dp),
        enabled = enabled,
        colors = androidx.compose.material3.ButtonDefaults.buttonColors(containerColor = accent),
        shape = MaterialTheme.shapes.medium,
    ) {
        Icon(
            painterResource(if (cancel) R.drawable.ms_close else R.drawable.ms_auto_awesome),
            contentDescription = null,
        )
        Spacer(Modifier.width(8.dp))
        Text(label)
    }
}

@file:OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock

// ---------------------------------------------------------------------------
// Input node body
// ---------------------------------------------------------------------------

@Composable
internal fun InputNodeBody(
    block: ProcessingBlock,
    lang: String,
    presetType: dev.screengoated.toolbox.mobile.shared.preset.PresetType,
    onUpdate: (ProcessingBlock) -> Unit,
) {
    val isTextInput = presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT ||
        presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT
    val isAudio = presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC ||
        presetType == dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO

    // Show overlay toggle
    SheetSwitchRow(
        icon = R.drawable.ms_visibility,
        label = nodeGraphLocalized(lang, "Show overlay", "Hiện overlay", "오버레이 표시"),
        checked = block.showOverlay,
        onCheckedChange = { onUpdate(block.copy(showOverlay = it)) },
    )

    // Render mode (only when overlay visible)
    AnimatedVisibility(
        visible = block.showOverlay,
        enter = fadeIn() + expandVertically(),
        exit = fadeOut() + shrinkVertically(),
    ) {
        InputRenderModeSelector(block, lang, onUpdate)
    }

    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.3f))

    // Auto-copy: text input = locked ON; image = toggleable; audio = hidden
    if (!isAudio) {
        SheetSwitchRow(
            icon = R.drawable.ms_content_copy,
            label = if (isTextInput) {
                nodeGraphLocalized(lang, "Auto-copy (always on)", "Tự sao chép (luôn bật)", "자동 복사 (항상 켜짐)")
            } else {
                nodeGraphLocalized(lang, "Auto-copy", "Tự sao chép", "자동 복사")
            },
            checked = if (isTextInput) true else block.autoCopy,
            onCheckedChange = {
                if (!isTextInput) onUpdate(block.copy(autoCopy = it))
                // Text input: locked on, ignore toggle
            },
            enabled = !isTextInput,
        )
    }

    // Auto-speak: only for text input presets
    if (isTextInput) {
        SheetSwitchRow(
            icon = R.drawable.ms_volume_up,
            label = nodeGraphLocalized(lang, "Auto-speak", "Tự phát âm", "자동 말하기"),
            checked = block.autoSpeak,
            onCheckedChange = { onUpdate(block.copy(autoSpeak = it)) },
        )
    }
}

@Composable
internal fun InputRenderModeSelector(
    block: ProcessingBlock,
    lang: String,
    onUpdate: (ProcessingBlock) -> Unit,
) {
    val options = listOf(
        RenderModeOption(nodeGraphLocalized(lang, "Normal", "Thường", "일반"), "plain", false),
        RenderModeOption(nodeGraphLocalized(lang, "Markdown", "Đẹp", "마크다운"), "markdown", false),
    )
    val currentIdx = if (block.renderMode == "markdown" || block.renderMode == "markdown_stream") 1 else 0

    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SheetLabel(nodeGraphLocalized(lang, "Render mode", "Chế độ hiển thị", "렌더링 모드"))
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
        ) {
            options.forEachIndexed { idx, opt ->
                ToggleButton(
                    checked = currentIdx == idx,
                    onCheckedChange = {
                        if (it) onUpdate(block.copy(renderMode = opt.renderMode, streamingEnabled = opt.streaming))
                    },
                    shapes = when (idx) {
                        0 -> ButtonGroupDefaults.connectedLeadingButtonShapes()
                        else -> ButtonGroupDefaults.connectedTrailingButtonShapes()
                    },
                    modifier = Modifier.weight(1f).semantics { role = Role.RadioButton },
                ) {
                    Text(opt.label, style = MaterialTheme.typography.labelMedium)
                }
            }
        }
    }
}

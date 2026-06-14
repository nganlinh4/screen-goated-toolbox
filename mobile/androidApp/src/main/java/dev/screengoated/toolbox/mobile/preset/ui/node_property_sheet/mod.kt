@file:OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock

// ---------------------------------------------------------------------------
// Render mode model
// ---------------------------------------------------------------------------

internal data class RenderModeOption(
    val label: String,
    val renderMode: String,
    val streaming: Boolean,
)

// ---------------------------------------------------------------------------
// Main bottom sheet
// ---------------------------------------------------------------------------

@Composable
fun NodePropertySheet(
    block: ProcessingBlock,
    nodeId: String,
    lang: String,
    presetType: dev.screengoated.toolbox.mobile.shared.preset.PresetType =
        dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT,
    onDismiss: () -> Unit,
    onBlockUpdated: (ProcessingBlock) -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var editBlock by remember(nodeId) { mutableStateOf(block) }

    // Propagate edits
    fun update(newBlock: ProcessingBlock) {
        editBlock = newBlock
        onBlockUpdated(newBlock)
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp)
                .padding(bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            // -- Header --
            NodeSheetHeader(editBlock, lang)

            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.3f))

            when (editBlock.blockType) {
                BlockType.INPUT_ADAPTER -> InputNodeBody(editBlock, lang, presetType, ::update)
                else -> ProcessNodeBody(editBlock, lang, ::update)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sheet header
// ---------------------------------------------------------------------------

@Composable
private fun NodeSheetHeader(block: ProcessingBlock, lang: String) {
    val accentColor = when (block.blockType) {
        BlockType.INPUT_ADAPTER -> Color(0xFF26A69A)
        BlockType.IMAGE -> Color(0xFFFFA726)
        BlockType.TEXT -> Color(0xFF42A5F5)
        BlockType.AUDIO -> Color(0xFFAB47BC)
    }
    val typeLabel = when (block.blockType) {
        BlockType.INPUT_ADAPTER -> nodeGraphLocalized(lang, "Input Node", "Nút đầu vào", "입력 노드")
        BlockType.IMAGE -> nodeGraphLocalized(lang, "Special Node (Image)", "Nút đặc biệt (Ảnh)", "특수 노드 (이미지)")
        BlockType.TEXT -> nodeGraphLocalized(lang, "Process Node", "Nút xử lý", "처리 노드")
        BlockType.AUDIO -> nodeGraphLocalized(lang, "Special Node (Audio)", "Nút đặc biệt (Âm thanh)", "특수 노드 (오디오)")
    }

    Row(verticalAlignment = Alignment.CenterVertically) {
        Surface(
            modifier = Modifier.size(8.dp),
            shape = CircleShape,
            color = accentColor,
            content = {},
        )
        Spacer(Modifier.width(10.dp))
        Text(
            typeLabel,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
            color = MaterialTheme.colorScheme.onSurface,
        )
    }
}

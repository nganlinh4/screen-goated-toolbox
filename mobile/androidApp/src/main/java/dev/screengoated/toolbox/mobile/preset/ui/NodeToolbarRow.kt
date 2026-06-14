package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock

@Composable
internal fun NodeToolbarRow(
    block: ProcessingBlock,
    lang: String,
    contentCol: Color,
    pillBg: Color,
    streamingActive: Boolean,
    onBlockUpdated: (ProcessingBlock) -> Unit,
) {
    var showRenderModeMenu by remember { mutableStateOf(false) }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        // Eye toggle
        androidx.compose.material3.IconToggleButton(
            checked = block.showOverlay,
            onCheckedChange = { onBlockUpdated(block.copy(showOverlay = it)) },
            modifier = Modifier.size(24.dp),
        ) {
            Icon(
                painter = painterResource(if (block.showOverlay) R.drawable.ms_visibility else R.drawable.ms_visibility_off),
                contentDescription = null,
                modifier = Modifier.size(14.dp),
            )
        }

        // Stream mode toggle pill (mobile always uses markdown)
        if (block.showOverlay) {
            val isStreaming = streamingActive
            val streamLabel = nodeGraphStreamLabel(lang, isStreaming)
            Box {
                Surface(
                    shape = RoundedCornerShape(4.dp),
                    color = pillBg,
                    modifier = Modifier.height(20.dp)
                        .pointerInput(Unit) { detectTapGestures { showRenderModeMenu = true } },
                ) {
                    Text(
                        streamLabel,
                        modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                        style = MaterialTheme.typography.labelSmall,
                        fontSize = 9.sp,
                        color = contentCol,
                    )
                }
                androidx.compose.material3.DropdownMenu(
                    expanded = showRenderModeMenu,
                    onDismissRequest = { showRenderModeMenu = false },
                ) {
                    listOf(
                        nodeGraphStreamLabel(lang, false) to false,
                        nodeGraphStreamLabel(lang, true) to true,
                    ).forEach { (label, streaming) ->
                        androidx.compose.material3.DropdownMenuItem(
                            text = { Text(label, style = MaterialTheme.typography.bodySmall) },
                            onClick = {
                                val mode = if (streaming) "markdown_stream" else "markdown"
                                onBlockUpdated(block.copy(renderMode = mode, streamingEnabled = streaming))
                                showRenderModeMenu = false
                            },
                        )
                    }
                }
            }
        }

        Spacer(Modifier.weight(1f))

        // Copy toggle (distinct icons for on/off like eye)
        androidx.compose.material3.IconToggleButton(
            checked = block.autoCopy,
            onCheckedChange = { onBlockUpdated(block.copy(autoCopy = it)) },
            modifier = Modifier.size(24.dp),
        ) {
            Icon(
                imageVector = if (block.autoCopy) FileCopyIcon
                    else FileCopyOffIcon,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
            )
        }

        // Speak toggle (distinct icons for on/off like eye)
        androidx.compose.material3.IconToggleButton(
            checked = block.autoSpeak,
            onCheckedChange = { onBlockUpdated(block.copy(autoSpeak = it)) },
            modifier = Modifier.size(24.dp),
        ) {
            Icon(
                painter = painterResource(if (block.autoSpeak) R.drawable.ms_volume_up else R.drawable.ms_volume_off),
                contentDescription = null,
                modifier = Modifier.size(14.dp),
            )
        }
    }
}

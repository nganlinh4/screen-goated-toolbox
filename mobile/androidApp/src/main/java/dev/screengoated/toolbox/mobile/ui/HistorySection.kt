@file:OptIn(
    androidx.compose.foundation.layout.ExperimentalLayoutApi::class,
    androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class,
)
@file:Suppress("DEPRECATION")

package dev.screengoated.toolbox.mobile.ui

import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.TextSnippet
import androidx.compose.material.icons.automirrored.rounded.OpenInNew
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material.icons.rounded.Folder
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.History
import androidx.compose.material.icons.rounded.Image
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ButtonGroup
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.history.HistoryExternalActions
import dev.screengoated.toolbox.mobile.history.HistoryItem
import dev.screengoated.toolbox.mobile.history.HistoryType
import dev.screengoated.toolbox.mobile.history.HistoryUiState
import dev.screengoated.toolbox.mobile.history.MAX_HISTORY_LIMIT
import dev.screengoated.toolbox.mobile.history.MIN_HISTORY_LIMIT
import dev.screengoated.toolbox.mobile.history.filterHistoryItems
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import java.io.File
import kotlin.math.roundToInt

private val historyActionButtonPadding = PaddingValues(horizontal = 8.dp, vertical = 8.dp)

@Composable
internal fun HistorySection(
    state: HistoryUiState,
    searchQuery: String,
    locale: MobileLocaleText,
    onSearchQueryChanged: (String) -> Unit,
    onClearSearchQuery: () -> Unit,
    onMaxItemsChanged: (Int) -> Unit,
    onDeleteItem: (Long) -> Unit,
    onClearAll: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val filteredItems = filterHistoryItems(state.items, searchQuery)

    LazyColumn(
        modifier = modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Card(
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
                ),
                shape = MaterialTheme.shapes.small,
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                        ) {
                            Icon(Icons.Rounded.History, contentDescription = null)
                            Text(
                                text = locale.historyTitle,
                                style = MaterialTheme.typography.titleMedium,
                                fontWeight = FontWeight.SemiBold,
                            )
                        }
                        Text(
                            text = "${locale.historyMaxItemsLabel} ${state.maxItems}",
                            style = MaterialTheme.typography.labelLarge,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }

                    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Slider(
                            value = state.maxItems.toFloat(),
                            onValueChange = { onMaxItemsChanged(it.roundToInt()) },
                            valueRange = MIN_HISTORY_LIMIT.toFloat()..MAX_HISTORY_LIMIT.toFloat(),
                        )
                        Text(
                            text = locale.historyRetentionHint,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        OutlinedTextField(
                            modifier = Modifier.weight(1f),
                            value = searchQuery,
                            onValueChange = onSearchQueryChanged,
                            singleLine = true,
                            label = { Text(locale.historySearchLabel) },
                            placeholder = { Text(locale.historySearchPlaceholder) },
                        )
                        if (searchQuery.isNotBlank()) {
                            IconButton(onClick = onClearSearchQuery) {
                                Icon(Icons.Rounded.Close, contentDescription = locale.historyClearSearch)
                            }
                        }
                    }

                    FlowRow(
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        FilledTonalButton(
                            onClick = {
                                val opened = HistoryExternalActions.openFolder(
                                    context = context,
                                    folder = File(state.mediaDirectoryPath.orEmpty()),
                                    supportsFolderOpen = state.supportsFolderOpen,
                                )
                                if (!opened) {
                                    Toast.makeText(
                                        context,
                                        locale.historyFolderUnavailable,
                                        Toast.LENGTH_SHORT,
                                    ).show()
                                }
                            },
                        ) {
                            Icon(Icons.Rounded.Folder, contentDescription = null, modifier = Modifier.size(18.dp))
                            Spacer(Modifier.size(6.dp))
                            Text(locale.historyOpenFolder)
                        }
                        Button(
                            onClick = onClearAll,
                            colors = ButtonDefaults.buttonColors(
                                containerColor = MaterialTheme.colorScheme.errorContainer,
                                contentColor = MaterialTheme.colorScheme.onErrorContainer,
                            ),
                        ) {
                            Icon(Icons.Rounded.Delete, contentDescription = null, modifier = Modifier.size(18.dp))
                            Spacer(Modifier.size(6.dp))
                            Text(locale.historyClearAll)
                        }
                    }
                }
            }
        }

        if (filteredItems.isEmpty()) {
            item {
                Card(
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
                    ),
                    shape = MaterialTheme.shapes.small,
                ) {
                    Text(
                        text = locale.historyEmpty,
                        modifier = Modifier.padding(20.dp),
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        } else {
            items(filteredItems, key = { it.id }) { item ->
                HistoryItemCard(
                    item = item,
                    mediaDirectoryPath = state.mediaDirectoryPath,
                    locale = locale,
                    onCopy = {
                        clipboard.setText(AnnotatedString(item.text))
                        Toast.makeText(context, locale.historyCopiedText, Toast.LENGTH_SHORT).show()
                    },
                    onOpen = {
                        val file = state.mediaDirectoryPath
                            ?.takeIf { item.mediaPath.isNotBlank() }
                            ?.let { File(it, item.mediaPath) }
                        if (file == null || !HistoryExternalActions.openItem(context, file)) {
                            Toast.makeText(context, locale.historyOpenFailed, Toast.LENGTH_SHORT).show()
                        }
                    },
                    onDelete = { onDeleteItem(item.id) },
                )
            }
        }
    }
}

@Composable
private fun HistoryItemCard(
    item: HistoryItem,
    mediaDirectoryPath: String?,
    locale: MobileLocaleText,
    onCopy: () -> Unit,
    onOpen: () -> Unit,
    onDelete: () -> Unit,
) {
    val cardColors = historyColors(item.itemType)
    val copyInteractionSource = remember { MutableInteractionSource() }
    val openInteractionSource = remember { MutableInteractionSource() }
    val deleteInteractionSource = remember { MutableInteractionSource() }
    Card(
        colors = CardDefaults.cardColors(
            containerColor = cardColors.containerColor,
            contentColor = cardColors.contentColor,
        ),
        shape = MaterialTheme.shapes.small,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Icon(
                        historyIcon(item.itemType),
                        contentDescription = null,
                        tint = cardColors.metaColor,
                    )
                    Text(
                        text = item.timestamp,
                        style = MaterialTheme.typography.labelMedium,
                        color = cardColors.metaColor,
                    )
                }
            }

            Text(
                text = item.text,
                style = MaterialTheme.typography.bodyMedium,
                color = cardColors.contentColor,
                maxLines = 6,
                overflow = TextOverflow.Ellipsis,
            )

            HorizontalDivider(color = cardColors.dividerColor)

            val hasOpenAction = !mediaDirectoryPath.isNullOrBlank() && item.mediaPath.isNotBlank()
            ButtonGroup(
                modifier = Modifier.fillMaxWidth(),
            ) {
                FilledTonalButton(
                    onClick = onCopy,
                    modifier = Modifier
                        .weight(1.05f)
                        .animateWidth(copyInteractionSource),
                    interactionSource = copyInteractionSource,
                    contentPadding = historyActionButtonPadding,
                ) {
                    Icon(Icons.Rounded.ContentCopy, contentDescription = null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.size(4.dp))
                    Text(
                        text = locale.historyCopyText,
                        style = MaterialTheme.typography.labelMediumEmphasized,
                        maxLines = 1,
                        softWrap = false,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                if (hasOpenAction) {
                    FilledTonalButton(
                        onClick = onOpen,
                        modifier = Modifier
                            .weight(0.95f)
                            .animateWidth(openInteractionSource),
                        interactionSource = openInteractionSource,
                        contentPadding = historyActionButtonPadding,
                    ) {
                        Icon(Icons.AutoMirrored.Rounded.OpenInNew, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.size(4.dp))
                        Text(
                            text = historyOpenLabel(item.itemType, locale),
                            style = MaterialTheme.typography.labelMediumEmphasized,
                            maxLines = 1,
                            softWrap = false,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
                Button(
                    onClick = onDelete,
                    modifier = Modifier
                        .weight(0.55f)
                        .animateWidth(deleteInteractionSource),
                    interactionSource = deleteInteractionSource,
                    contentPadding = historyActionButtonPadding,
                    colors = ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.errorContainer,
                        contentColor = MaterialTheme.colorScheme.onErrorContainer,
                    ),
                ) {
                    Icon(
                        Icons.Rounded.Delete,
                        contentDescription = locale.historyDelete,
                        modifier = Modifier.size(18.dp),
                    )
                }
            }
        }
    }
}

@Composable
private fun historyColors(type: HistoryType): HistoryCardColors {
    val colorScheme = MaterialTheme.colorScheme
    val isDark = colorScheme.surface.luminance() < 0.5f
    val baseSurface = if (isDark) colorScheme.surfaceContainerHigh else colorScheme.surfaceContainerLow
    val accentContainer = when (type) {
        HistoryType.IMAGE -> colorScheme.secondaryContainer
        HistoryType.AUDIO -> colorScheme.tertiaryContainer
        HistoryType.TEXT -> colorScheme.primaryContainer
    }
    val accentContent = when (type) {
        HistoryType.IMAGE -> colorScheme.onSecondaryContainer
        HistoryType.AUDIO -> colorScheme.onTertiaryContainer
        HistoryType.TEXT -> colorScheme.onPrimaryContainer
    }
    val containerColor = lerp(
        baseSurface,
        accentContainer,
        if (isDark) 0.12f else 0.62f,
    )
    val contentColor = if (isDark) {
        colorScheme.onSurface
    } else {
        lerp(colorScheme.onSurface, accentContent, 0.5f)
    }
    val metaColor = if (isDark) {
        colorScheme.onSurfaceVariant
    } else {
        lerp(contentColor, colorScheme.onSurfaceVariant, 0.28f)
    }
    val dividerColor = if (isDark) {
        contentColor.copy(alpha = 0.16f)
    } else {
        contentColor.copy(alpha = 0.14f)
    }
    return HistoryCardColors(
        containerColor = containerColor,
        contentColor = contentColor,
        metaColor = metaColor,
        dividerColor = dividerColor,
    )
}

private data class HistoryCardColors(
    val containerColor: Color,
    val contentColor: Color,
    val metaColor: Color,
    val dividerColor: Color,
)

private fun historyIcon(type: HistoryType) = when (type) {
    HistoryType.IMAGE -> Icons.Rounded.Image
    HistoryType.AUDIO -> Icons.Rounded.GraphicEq
    HistoryType.TEXT -> Icons.AutoMirrored.Rounded.TextSnippet
}

private fun historyOpenLabel(
    type: HistoryType,
    locale: MobileLocaleText,
): String {
    return when (type) {
        HistoryType.IMAGE -> locale.historyViewImage
        HistoryType.AUDIO -> locale.historyListenAudio
        HistoryType.TEXT -> locale.historyViewText
    }
}

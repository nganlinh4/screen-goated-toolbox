@file:OptIn(
    androidx.compose.foundation.layout.ExperimentalLayoutApi::class,
    androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class,
)
@file:Suppress("DEPRECATION")

package dev.screengoated.toolbox.mobile.ui

import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
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
            ExpressiveSettingsCard(
                accent = MaterialTheme.colorScheme.primary,
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth(),
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
                            HistorySectionHeroBadge(modifier = Modifier.size(42.dp))
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
                            Box(
                                modifier = Modifier
                                    .size(36.dp)
                                    .background(
                                        color = MaterialTheme.colorScheme.surfaceContainerHighest,
                                        shape = MaterialTheme.shapes.medium,
                                    ),
                                contentAlignment = Alignment.Center,
                            ) {
                                IconButton(onClick = onClearSearchQuery) {
                                    Icon(Icons.Rounded.Close, contentDescription = locale.historyClearSearch)
                                }
                            }
                        }
                    }

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        HistoryActionButton(
                            text = locale.historyOpenFolder,
                            icon = Icons.Rounded.Folder,
                            role = HistoryActionRole.FOLDER,
                            iconRotationDegrees = -90f,
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
                            modifier = Modifier.weight(1f),
                        )
                        HistoryActionButton(
                            text = locale.historyClearAll,
                            icon = Icons.Rounded.Delete,
                            role = HistoryActionRole.DELETE,
                            onClick = onClearAll,
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        }

        if (filteredItems.isEmpty()) {
            item {
                ExpressiveSettingsCard(
                    accent = MaterialTheme.colorScheme.outline,
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        HistorySectionHeroBadge(modifier = Modifier.size(38.dp))
                        Text(
                            text = locale.historyEmpty,
                            style = MaterialTheme.typography.bodyLarge,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
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
    Card(
        colors = CardDefaults.cardColors(
            containerColor = cardColors.containerColor,
            contentColor = cardColors.contentColor,
        ),
        shape = MaterialTheme.shapes.medium,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
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
                    HistoryTypeBadge(
                        type = item.itemType,
                        modifier = Modifier.size(34.dp),
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
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                HistoryActionButton(
                    text = locale.historyCopyText,
                    icon = Icons.Rounded.ContentCopy,
                    role = HistoryActionRole.COPY,
                    onClick = onCopy,
                    modifier = Modifier.weight(if (hasOpenAction) 1.18f else 1f),
                )
                if (hasOpenAction) {
                    HistoryActionButton(
                        text = historyOpenLabel(item.itemType, locale),
                        icon = Icons.AutoMirrored.Rounded.OpenInNew,
                        role = HistoryActionRole.OPEN,
                        onClick = onOpen,
                        modifier = Modifier.weight(1.08f),
                    )
                }
                HistoryDeleteAction(
                    onClick = onDelete,
                    contentDescription = locale.historyDelete,
                )
            }
        }
    }
}

@Composable
private fun HistoryDeleteAction(
    onClick: () -> Unit,
    contentDescription: String,
    modifier: Modifier = Modifier,
) {
    IconButton(
        onClick = onClick,
        modifier = modifier.size(44.dp),
    ) {
        Icon(
            imageVector = Icons.Rounded.Delete,
            contentDescription = contentDescription,
            tint = MaterialTheme.colorScheme.error,
            modifier = Modifier.size(24.dp),
        )
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

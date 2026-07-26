package dev.screengoated.toolbox.mobile.creation

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed as gridItemsIndexed
import androidx.compose.foundation.lazy.itemsIndexed as rowItemsIndexed
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.UtilityExpressiveCard
import dev.screengoated.toolbox.mobile.ui.UtilityHeaderRow
import dev.screengoated.toolbox.mobile.ui.i18n.CreationImageLocale
import java.io.File

@Composable
internal fun CreationImageSettings(
    item: CreationNativeItem,
    strings: CreationImageLocale,
    accent: Color,
    enabled: Boolean,
    onPrompt: (String) -> Unit,
    onAddReferences: () -> Unit,
    onRemoveReference: (Int) -> Unit,
) {
    UtilityExpressiveCard(accent = accent) {
        UtilityHeaderRow(
            icon = R.drawable.ms_image,
            title = strings.references,
            accent = accent,
        )
        Text(
            strings.referenceCount.replace("{}", item.referencePaths.size.toString()),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (item.referencePaths.isEmpty()) {
            Text(
                strings.noReferences,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                rowItemsIndexed(
                    items = item.referencePaths,
                    key = { index, path -> "$index:$path" },
                ) { index, path ->
                    Box(Modifier.size(82.dp)) {
                        CreationImageThumbnail(
                            path = path,
                            modifier = Modifier
                                .fillMaxSize()
                                .clip(MaterialTheme.shapes.medium),
                        )
                        if (enabled) {
                            IconButton(
                                onClick = { onRemoveReference(index) },
                                modifier = Modifier
                                    .align(Alignment.TopEnd)
                                    .size(30.dp)
                                    .background(
                                        MaterialTheme.colorScheme.surface.copy(alpha = 0.88f),
                                        MaterialTheme.shapes.small,
                                    ),
                            ) {
                                Icon(
                                    painterResource(R.drawable.ms_close),
                                    contentDescription = strings.removeReference,
                                    modifier = Modifier.size(16.dp),
                                )
                            }
                        }
                    }
                }
            }
        }
        if (
            enabled &&
            item.referencePaths.size < CreationContract.IMAGE_CREATOR_MAXIMUM_REFERENCE_IMAGES
        ) {
            OutlinedButton(onClick = onAddReferences) {
                Icon(painterResource(R.drawable.ms_add), contentDescription = null)
                Text(strings.addReferences, modifier = Modifier.padding(start = 8.dp))
            }
        }
    }
    UtilityExpressiveCard(accent = accent) {
        UtilityHeaderRow(
            icon = R.drawable.ms_edit,
            title = strings.instruction,
            accent = accent,
        )
        OutlinedTextField(
            value = item.prompt,
            onValueChange = onPrompt,
            modifier = Modifier.fillMaxWidth(),
            enabled = enabled,
            minLines = 4,
            maxLines = 8,
            placeholder = { Text(strings.instructionHint) },
            supportingText = {
                Text(
                    if (item.prompt.isBlank()) {
                        strings.promptRequired
                    } else {
                        "${item.prompt.length} / " +
                            CreationContract.IMAGE_CREATOR_MAXIMUM_PROMPT_CHARACTERS
                    },
                )
            },
        )
    }
}

@Composable
internal fun CreationImageSource(
    referencePaths: List<String>,
    strings: CreationImageLocale,
    accent: Color,
) {
    when (referencePaths.size) {
        0 -> Column(
            modifier = Modifier.fillMaxSize().padding(28.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Icon(
                painterResource(R.drawable.ms_auto_awesome),
                contentDescription = null,
                tint = accent,
                modifier = Modifier.size(42.dp),
            )
            Text(
                strings.textOnlyTitle,
                modifier = Modifier.padding(top = 12.dp),
                style = MaterialTheme.typography.titleMedium,
            )
            Text(
                strings.textOnlyHint,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodyMedium,
            )
        }
        1 -> CreationImageThumbnail(
            path = referencePaths.single(),
            contentScale = ContentScale.Fit,
            modifier = Modifier.fillMaxSize().padding(12.dp),
        )
        else -> LazyVerticalGrid(
            columns = GridCells.Adaptive(112.dp),
            modifier = Modifier.fillMaxSize().padding(10.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            gridItemsIndexed(
                items = referencePaths,
                key = { index, path -> "$index:$path" },
            ) { _, path ->
                CreationImageThumbnail(
                    path = path,
                    contentScale = ContentScale.Fit,
                    modifier = Modifier.height(112.dp).clip(MaterialTheme.shapes.medium),
                )
            }
        }
    }
}

@Composable
internal fun CreationImageResult(
    referencePaths: List<String>,
    outputPath: String,
    viewModel: CreationNativeViewModel,
    strings: CreationImageLocale,
) {
    val outputFile by produceState<File?>(null, outputPath) {
        value = runCatching { viewModel.previewFile(outputPath, "png") }.getOrNull()
    }
    BoxWithConstraints(Modifier.fillMaxSize().padding(12.dp)) {
        when {
            referencePaths.isEmpty() -> CreationImagePanel(
                strings.after,
                outputFile?.absolutePath,
                Modifier.fillMaxSize(),
            )
            referencePaths.size == 1 && maxWidth >= 520.dp -> Row(
                modifier = Modifier.fillMaxSize(),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                CreationImagePanel(strings.before, referencePaths.single(), Modifier.weight(1f))
                CreationImagePanel(
                    strings.after,
                    outputFile?.absolutePath,
                    Modifier.weight(1f),
                )
            }
            referencePaths.size == 1 -> Column(
                modifier = Modifier.fillMaxSize(),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                CreationImagePanel(strings.before, referencePaths.single(), Modifier.weight(1f))
                CreationImagePanel(
                    strings.after,
                    outputFile?.absolutePath,
                    Modifier.weight(1f),
                )
            }
            else -> Column(
                modifier = Modifier.fillMaxSize(),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                CreationImagePanel(
                    strings.after,
                    outputFile?.absolutePath,
                    Modifier.weight(1f),
                )
                Text(strings.references, style = MaterialTheme.typography.labelMedium)
                LazyRow(horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                    rowItemsIndexed(referencePaths) { index, path ->
                        CreationImageThumbnail(
                            path = path,
                            contentScale = ContentScale.Fit,
                            modifier = Modifier
                                .size(72.dp)
                                .clip(MaterialTheme.shapes.small),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun CreationImagePanel(label: String, path: String?, modifier: Modifier) {
    Surface(
        modifier = modifier,
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerHighest,
    ) {
        Column {
            Text(
                text = label,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (path != null) {
                CreationImageThumbnail(
                    path = path,
                    contentScale = ContentScale.Fit,
                    modifier = Modifier.fillMaxWidth().weight(1f),
                )
            }
        }
    }
}

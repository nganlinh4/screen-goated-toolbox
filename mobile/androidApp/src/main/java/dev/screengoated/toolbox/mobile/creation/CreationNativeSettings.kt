@file:OptIn(androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.creation

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.material3.ToggleButtonColors
import androidx.compose.material3.ToggleButtonDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.UtilityExpressiveCard
import dev.screengoated.toolbox.mobile.ui.UtilityHeaderRow
import dev.screengoated.toolbox.mobile.ui.i18n.Creation3dLocale
import dev.screengoated.toolbox.mobile.ui.i18n.CreationCommonLocale
import dev.screengoated.toolbox.mobile.ui.i18n.CreationSvgLocale
import java.text.NumberFormat

@Composable
internal fun Creation3dSettings(
    item: CreationNativeItem,
    strings: Creation3dLocale,
    accent: Color,
    enabled: Boolean,
    onGenerationMode: (String) -> Unit,
    onPolycount: (Int) -> Unit,
    onAutoSegment: (Boolean) -> Unit,
    onInstruction: (String) -> Unit,
) {
    val mode = CreationGenerationMode.fromWireName(item.generationMode)
    val route = CreationContract.route3dMode(mode, item.polycount, item.autoSegment)
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        UtilityExpressiveCard(accent = accent) {
            UtilityHeaderRow(
                icon = R.drawable.ms_auto_awesome,
                title = strings.mode,
                accent = accent,
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(
                    ButtonGroupDefaults.ConnectedSpaceBetween,
                ),
            ) {
                ModeToggle(
                    selected = mode == CreationGenerationMode.FAST,
                    label = strings.fast,
                    enabled = enabled,
                    shapes = ButtonGroupDefaults.connectedLeadingButtonShapes(),
                    accent = accent,
                    onClick = { onGenerationMode(CreationGenerationMode.FAST.wireName) },
                    modifier = Modifier
                        .weight(1f)
                        .testTag("creation-mode-fast"),
                )
                ModeToggle(
                    selected = mode == CreationGenerationMode.QUALITY,
                    label = strings.quality,
                    enabled = enabled,
                    shapes = ButtonGroupDefaults.connectedTrailingButtonShapes(),
                    accent = accent,
                    onClick = { onGenerationMode(CreationGenerationMode.QUALITY.wireName) },
                    modifier = Modifier
                        .weight(1f)
                        .testTag("creation-mode-quality"),
                )
            }
        }
        if (item.allowsInstruction) {
            UtilityExpressiveCard(accent = accent) {
                UtilityHeaderRow(
                    icon = R.drawable.ms_edit,
                    title = strings.instruction,
                    accent = accent,
                )
                OutlinedTextField(
                    value = item.instruction,
                    onValueChange = onInstruction,
                    enabled = enabled,
                    modifier = Modifier.fillMaxWidth(),
                    minLines = 2,
                    maxLines = 5,
                    placeholder = { Text(strings.instructionHint) },
                )
            }
        }
        UtilityExpressiveCard(accent = accent) {
            UtilityHeaderRow(
                icon = R.drawable.ms_tune,
                title = strings.polycount,
                accent = accent,
                trailing = {
                    Text(
                        NumberFormat.getIntegerInstance().format(route.polycount),
                        style = MaterialTheme.typography.titleSmall,
                        color = accent,
                    )
                },
            )
            Slider(
                value = route.polycount.toFloat(),
                onValueChange = { onPolycount((it / 100f).toInt() * 100) },
                valueRange = when (mode) {
                    CreationGenerationMode.FAST ->
                        CreationContract.MINIMUM_POLYCOUNT.toFloat()..
                            CreationContract.FAST_MAXIMUM_POLYCOUNT.toFloat()
                    CreationGenerationMode.QUALITY ->
                        CreationContract.QUALITY_MINIMUM_POLYCOUNT.toFloat()..
                            CreationContract.MAXIMUM_POLYCOUNT.toFloat()
                },
                enabled = enabled,
                modifier = Modifier.fillMaxWidth(),
            )
            Row(modifier = Modifier.fillMaxWidth()) {
                Text(
                    strings.light,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.weight(1f))
                Text(
                    strings.detailed,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (route.showAutoSegment) {
                HorizontalDivider()
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable(enabled = enabled) { onAutoSegment(!item.autoSegment) }
                        .semantics { role = Role.Switch }
                        .padding(vertical = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Icon(
                        painterResource(R.drawable.ms_layers),
                        contentDescription = null,
                        tint = accent,
                        modifier = Modifier.size(21.dp),
                    )
                    Text(
                        strings.autoSeparate,
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.weight(1f),
                    )
                    Switch(
                        checked = item.autoSegment,
                        onCheckedChange = onAutoSegment,
                        enabled = enabled,
                    )
                }
            }
        }
    }
}

@Composable
private fun ModeToggle(
    selected: Boolean,
    label: String,
    enabled: Boolean,
    shapes: androidx.compose.material3.ToggleButtonShapes,
    accent: Color,
    onClick: () -> Unit,
    modifier: Modifier,
) {
    ToggleButton(
        checked = selected,
        onCheckedChange = { if (it) onClick() },
        enabled = enabled,
        shapes = shapes,
        colors = modelToggleColors(selected, accent),
        modifier = modifier,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.labelLarge,
            color = modelToggleTextColor(selected, accent),
        )
    }
}

@Composable
internal fun CreationSvgSettings(
    item: CreationNativeItem,
    strings: CreationSvgLocale,
    accent: Color,
    enabled: Boolean,
    onModel: (String) -> Unit,
    onBackgroundMode: (String) -> Unit,
) {
    val simpleSelected = item.model != "detail"
    val detailSelected = !simpleSelected
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        UtilityExpressiveCard(accent = accent) {
            UtilityHeaderRow(
                icon = R.drawable.ms_auto_awesome,
                title = strings.model,
                accent = accent,
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement =
                    Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
            ) {
                ToggleButton(
                    checked = simpleSelected,
                    onCheckedChange = { if (it) onModel("simple") },
                    enabled = enabled,
                    shapes = ButtonGroupDefaults.connectedLeadingButtonShapes(),
                    colors = modelToggleColors(simpleSelected, accent),
                    modifier = Modifier
                        .weight(1f)
                        .testTag("creation-svg-simple"),
                ) {
                    Column(horizontalAlignment = Alignment.Start) {
                        Text(
                            strings.simple,
                            style = MaterialTheme.typography.labelLarge,
                            color = modelToggleTextColor(simpleSelected, accent),
                        )
                        Text(
                            strings.simpleDescription,
                            style = MaterialTheme.typography.labelSmall,
                            color = modelToggleTextColor(simpleSelected, accent).copy(alpha = 0.82f),
                            maxLines = 2,
                        )
                    }
                }
                ToggleButton(
                    checked = detailSelected,
                    onCheckedChange = { if (it) onModel("detail") },
                    enabled = enabled,
                    shapes = ButtonGroupDefaults.connectedTrailingButtonShapes(),
                    colors = modelToggleColors(detailSelected, accent),
                    modifier = Modifier.weight(1f),
                ) {
                    Column(horizontalAlignment = Alignment.Start) {
                        Text(
                            strings.detail,
                            style = MaterialTheme.typography.labelLarge,
                            color = modelToggleTextColor(detailSelected, accent),
                        )
                        Text(
                            strings.detailDescription,
                            style = MaterialTheme.typography.labelSmall,
                            color = modelToggleTextColor(detailSelected, accent).copy(alpha = 0.82f),
                            maxLines = 2,
                        )
                    }
                }
            }
        }
        UtilityExpressiveCard(accent = accent) {
            UtilityHeaderRow(
                icon = R.drawable.ms_image,
                title = strings.transparentBackground,
                accent = accent,
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement =
                    Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
            ) {
                listOf(
                    "auto" to strings.backgroundAuto,
                    "transparent" to strings.backgroundOn,
                    "opaque" to strings.backgroundOff,
                ).forEachIndexed { index, (mode, label) ->
                    val selected = item.backgroundMode == mode
                    ModeToggle(
                        selected = selected,
                        label = label,
                        enabled = enabled,
                        shapes = when (index) {
                            0 -> ButtonGroupDefaults.connectedLeadingButtonShapes()
                            2 -> ButtonGroupDefaults.connectedTrailingButtonShapes()
                            else -> ButtonGroupDefaults.connectedMiddleButtonShapes()
                        },
                        accent = accent,
                        onClick = { onBackgroundMode(mode) },
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
    }
}

@Composable
private fun modelToggleColors(selected: Boolean, accent: Color): ToggleButtonColors {
    if (!selected) return ToggleButtonDefaults.toggleButtonColors()
    val content = modelToggleTextColor(true, accent)
    return ToggleButtonDefaults.toggleButtonColors(
        disabledContainerColor = accent,
        disabledContentColor = content,
        checkedContainerColor = accent,
        checkedContentColor = content,
    )
}

@Composable
private fun modelToggleTextColor(selected: Boolean, accent: Color): Color = when {
    !selected -> MaterialTheme.colorScheme.onSurfaceVariant
    accent.luminance() > 0.42f -> Color.Black
    else -> Color.White
}

@Composable
internal fun CreationOutputSettings(
    outputDirectory: String,
    common: CreationCommonLocale,
    accent: Color,
    onChangeFolder: () -> Unit,
) {
    UtilityExpressiveCard(
        accent = accent,
        modifier = Modifier.clickable(onClick = onChangeFolder),
    ) {
        UtilityHeaderRow(
            icon = R.drawable.ms_folder,
            title = common.saveTo,
            supporting = outputDirectory,
            accent = accent,
            trailing = {
                IconButton(onClick = onChangeFolder) {
                    Icon(
                        painterResource(R.drawable.ms_edit),
                        contentDescription = common.changeFolder,
                    )
                }
            },
        )
    }
}

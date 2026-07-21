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
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
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
    onPolycount: (Int) -> Unit,
    onAutoSegment: (Boolean) -> Unit,
) {
    UtilityExpressiveCard(accent = accent) {
        UtilityHeaderRow(
            icon = R.drawable.ms_tune,
            title = strings.polycount,
            accent = accent,
            trailing = {
                Text(
                    NumberFormat.getIntegerInstance().format(item.polycount),
                    style = MaterialTheme.typography.titleSmall,
                    color = accent,
                )
            },
        )
        Slider(
            value = item.polycount.toFloat(),
            onValueChange = { onPolycount((it / 100f).toInt() * 100) },
            valueRange = CreationContract.MINIMUM_POLYCOUNT.toFloat()..
                CreationContract.MAXIMUM_POLYCOUNT.toFloat(),
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

@Composable
internal fun CreationSvgSettings(
    item: CreationNativeItem,
    strings: CreationSvgLocale,
    accent: Color,
    enabled: Boolean,
    onModel: (String) -> Unit,
) {
    UtilityExpressiveCard(accent = accent) {
        UtilityHeaderRow(
            icon = R.drawable.ms_auto_awesome,
            title = strings.model,
            accent = accent,
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
        ) {
            ToggleButton(
                checked = item.model != "detail",
                onCheckedChange = { if (it) onModel("simple") },
                enabled = enabled,
                shapes = ButtonGroupDefaults.connectedLeadingButtonShapes(),
                modifier = Modifier.weight(1f),
            ) {
                Column(horizontalAlignment = Alignment.Start) {
                    Text(strings.simple, style = MaterialTheme.typography.labelLarge)
                    Text(
                        strings.simpleDescription,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 2,
                    )
                }
            }
            ToggleButton(
                checked = item.model == "detail",
                onCheckedChange = { if (it) onModel("detail") },
                enabled = enabled,
                shapes = ButtonGroupDefaults.connectedTrailingButtonShapes(),
                modifier = Modifier.weight(1f),
            ) {
                Column(horizontalAlignment = Alignment.Start) {
                    Text(strings.detail, style = MaterialTheme.typography.labelLarge)
                    Text(
                        strings.detailDescription,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 2,
                    )
                }
            }
        }
    }
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

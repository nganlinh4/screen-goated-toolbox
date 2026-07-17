@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.downloader.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.downloader.DownloaderSettings

@Composable
internal fun DropdownSelector(
    label: String,
    options: List<String>,
    selected: String,
    onSelect: (String) -> Unit,
    header: String? = null,
) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        OutlinedButton(onClick = { expanded = true }) {
            Text("$label $selected", style = MaterialTheme.typography.bodySmall)
            Spacer(Modifier.width(4.dp))
            Icon(
                painterResource(if (expanded) R.drawable.ms_expand_less else R.drawable.ms_expand_more),
                contentDescription = null,
                Modifier.size(16.dp),
            )
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            if (header != null) {
                DropdownMenuItem(
                    text = {
                        Text(
                            header,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    },
                    onClick = {},
                    enabled = false,
                )
                HorizontalDivider()
            }
            options.forEach { opt ->
                DropdownMenuItem(
                    text = { Text(opt) },
                    onClick = { onSelect(opt); expanded = false },
                )
            }
        }
    }
}

@Composable
internal fun AdvancedSection(
    settings: DownloaderSettings,
    locale: dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText,
    onUpdate: ((DownloaderSettings) -> DownloaderSettings) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    Column {
        OutlinedButton(onClick = { expanded = !expanded }) {
            Icon(painterResource(R.drawable.ms_settings), contentDescription = null, Modifier.size(14.dp))
            Spacer(Modifier.width(4.dp))
            Text(locale.dlAdvanced, style = MaterialTheme.typography.labelSmall)
            Spacer(Modifier.width(2.dp))
            Icon(
                painterResource(if (expanded) R.drawable.ms_expand_less else R.drawable.ms_expand_more),
                contentDescription = null,
                Modifier.size(14.dp),
            )
        }
        AnimatedVisibility(visible = expanded) {
            Column(Modifier.padding(top = 8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                ToggleRow(locale.dlOptMetadata, settings.useMetadata) { onUpdate { s -> s.copy(useMetadata = it) } }
                ToggleRow(locale.dlOptSponsorblock, settings.useSponsorBlock) { onUpdate { s -> s.copy(useSponsorBlock = it) } }
                ToggleRow(locale.dlOptSubtitles, settings.useSubtitles) { onUpdate { s -> s.copy(useSubtitles = it) } }
                ToggleRow(locale.dlOptPlaylist, settings.usePlaylist) { onUpdate { s -> s.copy(usePlaylist = it) } }
            }
        }
    }
}

@Composable
internal fun ToggleRow(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, style = MaterialTheme.typography.bodySmall, modifier = Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

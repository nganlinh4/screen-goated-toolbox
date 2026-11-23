@file:OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.preset.PresetModelType
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun TypeBadge(label: String) {
    Text(
        text = label,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier
            .background(
                MaterialTheme.colorScheme.primary.copy(alpha = 0.14f),
                MaterialTheme.shapes.small,
            )
            .padding(horizontal = 8.dp, vertical = 3.dp),
    )
}

@Composable
internal fun CustomModelEditor(
    model: CustomPresetModelDefinition,
    locale: MobileLocaleText,
    accent: Color,
    onChange: ((CustomPresetModelDefinition) -> CustomPresetModelDefinition) -> Unit,
    onDelete: () -> Unit,
) {
    var typeExpanded by remember { mutableStateOf(false) }
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(
            containerColor = lerp(MaterialTheme.colorScheme.surfaceContainerLow, accent, 0.12f),
        ),
        border = BorderStroke(1.dp, accent.copy(alpha = 0.30f)),
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Checkbox(
                    checked = model.enabled,
                    onCheckedChange = { checked -> onChange { it.copy(enabled = checked) } },
                )
                Text(locale.customModelsEnabled)
                Checkbox(
                    checked = model.supportsSearch == true,
                    onCheckedChange = { checked -> onChange { it.copy(supportsSearch = checked) } },
                )
                Text(locale.customModelsSearch, modifier = Modifier.weight(1f))
                IconButton(onClick = onDelete) {
                    Icon(
                        painter = painterResource(R.drawable.ms_delete),
                        contentDescription = locale.customModelsDelete,
                        tint = MaterialTheme.colorScheme.error,
                    )
                }
            }
            OutlinedTextField(
                value = model.displayName,
                onValueChange = { value -> onChange { it.copy(displayName = value) } },
                label = { Text(locale.customModelsDisplayName) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = model.fullName,
                onValueChange = { value -> onChange { it.copy(fullName = value) } },
                label = { Text(locale.customModelsApiModel) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            ExposedDropdownMenuBox(
                expanded = typeExpanded,
                onExpandedChange = { typeExpanded = it },
            ) {
                OutlinedTextField(
                    value = modelTypeLabel(model.modelType, locale),
                    onValueChange = {},
                    readOnly = true,
                    label = { Text(locale.customModelsType) },
                    trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = typeExpanded) },
                    modifier = Modifier
                        .menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable, enabled = true)
                        .fillMaxWidth(),
                )
                ExposedDropdownMenu(
                    expanded = typeExpanded,
                    onDismissRequest = { typeExpanded = false },
                ) {
                    DropdownMenuItem(
                        text = { Text(locale.customModelsTextType) },
                        onClick = {
                            onChange { it.copy(modelType = PresetModelType.TEXT) }
                            typeExpanded = false
                        },
                    )
                    DropdownMenuItem(
                        text = { Text(locale.customModelsVisionType) },
                        onClick = {
                            onChange { it.copy(modelType = PresetModelType.VISION) }
                            typeExpanded = false
                        },
                    )
                }
            }
            Text(
                text = model.id,
                fontFamily = FontFamily.Monospace,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

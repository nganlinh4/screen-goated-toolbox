@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material.icons.rounded.KeyboardArrowUp
import androidx.compose.material.icons.rounded.Mic
import androidx.compose.material.icons.rounded.SpeakerPhone
import androidx.compose.material.icons.rounded.Star
import androidx.compose.material.icons.rounded.StarOutline
import androidx.compose.material.icons.rounded.TextFields
import androidx.compose.material.icons.rounded.Translate
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetType

private data class CategoryDef(
    val type: PresetType,
    val labelEn: String,
    val labelVi: String,
    val labelKo: String,
    val icon: ImageVector,
    val dotColor: Color,
    val presets: List<Preset>,
) {
    fun label(lang: String): String = when (lang) {
        "vi" -> labelVi
        "ko" -> labelKo
        else -> labelEn
    }
}

@Composable
fun PresetListSection(
    lang: String,
    onPresetClick: (String) -> Unit,
    onFavoriteToggle: (String) -> Unit,
) {
    val favorites = remember { mutableStateMapOf<String, Boolean>() }
    val sgtColors = MaterialTheme.sgtColors

    val categories = listOf(
        CategoryDef(
            type = PresetType.IMAGE,
            labelEn = "Image",
            labelVi = "Hình ảnh",
            labelKo = "이미지",
            icon = Icons.Rounded.Translate,
            dotColor = sgtColors.categoryImage,
            presets = DefaultPresets.imagePresets,
        ),
        CategoryDef(
            type = PresetType.TEXT_SELECT,
            labelEn = "Text Select",
            labelVi = "Chọn text",
            labelKo = "텍스트 선택",
            icon = Icons.Rounded.TextFields,
            dotColor = sgtColors.categoryTextSelect,
            presets = DefaultPresets.textSelectPresets,
        ),
        CategoryDef(
            type = PresetType.TEXT_INPUT,
            labelEn = "Text Input",
            labelVi = "Nhập text",
            labelKo = "텍스트 입력",
            icon = Icons.Rounded.TextFields,
            dotColor = sgtColors.categoryTextInput,
            presets = DefaultPresets.textInputPresets,
        ),
        CategoryDef(
            type = PresetType.MIC,
            labelEn = "Mic",
            labelVi = "Mic",
            labelKo = "마이크",
            icon = Icons.Rounded.Mic,
            dotColor = sgtColors.categoryMic,
            presets = DefaultPresets.micPresets,
        ),
        CategoryDef(
            type = PresetType.DEVICE_AUDIO,
            labelEn = "Device Audio",
            labelVi = "Âm thanh máy",
            labelKo = "기기 오디오",
            icon = Icons.Rounded.SpeakerPhone,
            dotColor = sgtColors.categoryDevice,
            presets = DefaultPresets.deviceAudioPresets,
        ),
    )

    Column(
        modifier = Modifier
            .fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        categories.forEach { category ->
            CategorySection(
                category = category,
                lang = lang,
                favorites = favorites,
                onPresetClick = onPresetClick,
                onFavoriteToggle = { presetId ->
                    val current = favorites[presetId] ?: false
                    favorites[presetId] = !current
                    onFavoriteToggle(presetId)
                },
            )
        }
    }
}

@Composable
private fun CategorySection(
    category: CategoryDef,
    lang: String,
    favorites: Map<String, Boolean>,
    onPresetClick: (String) -> Unit,
    onFavoriteToggle: (String) -> Unit,
) {
    val expanded = remember { mutableStateOf(true) }

    Column(modifier = Modifier.fillMaxWidth()) {
        // Category header
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable { expanded.value = !expanded.value }
                .padding(horizontal = 16.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Surface(
                shape = CircleShape,
                color = category.dotColor,
                modifier = Modifier.size(10.dp),
            ) {}
            Text(
                text = category.label(lang),
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f),
            )
            Text(
                text = "${category.presets.size}",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Icon(
                imageVector = if (expanded.value) {
                    Icons.Rounded.KeyboardArrowUp
                } else {
                    Icons.Rounded.KeyboardArrowDown
                },
                contentDescription = null,
                modifier = Modifier.size(20.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        // Collapsible preset list
        AnimatedVisibility(
            visible = expanded.value,
            enter = expandVertically(),
            exit = shrinkVertically(),
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 8.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                category.presets.forEach { preset ->
                    PresetItemCard(
                        preset = preset,
                        lang = lang,
                        icon = category.icon,
                        iconTint = category.dotColor,
                        isFavorite = favorites[preset.id] ?: false,
                        onClick = { onPresetClick(preset.id) },
                        onFavoriteToggle = { onFavoriteToggle(preset.id) },
                    )
                }
            }
        }
    }
}

@Composable
private fun PresetItemCard(
    preset: Preset,
    lang: String,
    icon: ImageVector,
    iconTint: Color,
    isFavorite: Boolean,
    onClick: () -> Unit,
    onFavoriteToggle: () -> Unit,
) {
    Card(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Category icon
            Box(
                modifier = Modifier.size(32.dp),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    modifier = Modifier.size(20.dp),
                    tint = iconTint,
                )
            }

            Spacer(modifier = Modifier.width(10.dp))

            // Preset name
            Text(
                text = preset.name(lang),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f),
            )

            // Favorite toggle
            IconButton(
                onClick = onFavoriteToggle,
                modifier = Modifier.size(36.dp),
            ) {
                Icon(
                    imageVector = if (isFavorite) {
                        Icons.Rounded.Star
                    } else {
                        Icons.Rounded.StarOutline
                    },
                    contentDescription = null,
                    modifier = Modifier.size(20.dp),
                    tint = if (isFavorite) {
                        MaterialTheme.sgtColors.favoriteStar
                    } else {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    },
                )
            }
        }
    }
}

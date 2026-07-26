@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.i18n.MobileUsageTipCategoryId
import dev.screengoated.toolbox.mobile.ui.i18n.MobileUsageTipCategoryText
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors

@Composable
internal fun UsageTipsCard(
    locale: MobileLocaleText,
    modifier: Modifier = Modifier,
) {
    var showDialog by rememberSaveable { mutableStateOf(false) }
    val categories = locale.usageTipsCategories.filter { it.tips.isNotEmpty() }
    val accent = MaterialTheme.sgtColors.appSlotAmber

    ExpressiveSettingsCard(
        accent = accent,
        modifier = modifier
            .fillMaxWidth()
            .clickable(enabled = categories.isNotEmpty()) { showDialog = true },
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            MorphingShapeBadge(
                morphPair = ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Cookie6Sided),
                progress = 0.56f,
                containerColor = accent.copy(alpha = 0.18f),
                modifier = Modifier.size(42.dp),
            ) {
                Icon(
                    painter = painterResource(R.drawable.ms_lightbulb),
                    contentDescription = null,
                    modifier = Modifier.size(20.dp),
                    tint = accent,
                )
            }
            Column(modifier = Modifier.weight(1f)) {
                SettingsCardTitle(
                    text = locale.usageTipsTitle,
                    maxLines = 2,
                )
                Text(
                    text = locale.usageTipsClickHint,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }

    if (showDialog && categories.isNotEmpty()) {
        UsageTipsDialog(
            locale = locale,
            categories = categories,
            onDismiss = { showDialog = false },
        )
    }
}

@Composable
private fun UsageTipsDialog(
    locale: MobileLocaleText,
    categories: List<MobileUsageTipCategoryText>,
    onDismiss: () -> Unit,
) {
    val accent = MaterialTheme.sgtColors.appSlotAmber
    ExpressiveDialogSurface(
        title = locale.usageTipsTitle,
        icon = R.drawable.ms_lightbulb,
        accent = accent,
        morphPair = ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Cookie6Sided),
        onDismiss = onDismiss,
        supporting = locale.usageTipsDescription,
        maxWidth = 560.dp,
        maxHeight = 620.dp,
        heightFraction = 0.76f,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            categories.forEach { category ->
                UsageTipCategorySection(category)
            }
        }
    }
}

@Composable
private fun UsageTipCategorySection(
    category: MobileUsageTipCategoryText,
) {
    val accent = usageTipCategoryAccent(category.id)
    val icon = usageTipCategoryIcon(category.id)
    val regularColor = MaterialTheme.colorScheme.onSurfaceVariant
    ExpressiveDialogSectionCard(accent = accent) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            MorphingShapeBadge(
                morphPair = ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Cookie4Sided),
                progress = 0.62f,
                containerColor = lerp(
                    MaterialTheme.colorScheme.surfaceContainerHighest,
                    accent,
                    0.2f,
                ),
                modifier = Modifier.size(42.dp),
            ) {
                Icon(
                    painter = painterResource(icon),
                    contentDescription = null,
                    tint = accent,
                    modifier = Modifier.size(20.dp),
                )
            }
            Column(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                Text(
                    text = category.title,
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    text = category.description,
                    style = MaterialTheme.typography.bodySmall,
                    color = regularColor,
                )
            }
        }
        Column(verticalArrangement = Arrangement.spacedBy(9.dp)) {
            category.tips.forEach { tip ->
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(
                            color = lerp(
                                MaterialTheme.colorScheme.surfaceContainerHighest,
                                accent,
                                0.055f,
                            ),
                            shape = MaterialTheme.shapes.medium,
                        )
                        .padding(horizontal = 12.dp, vertical = 11.dp),
                    verticalAlignment = Alignment.Top,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Box(
                        modifier = Modifier
                            .padding(top = 2.dp)
                            .size(width = 4.dp, height = 22.dp)
                            .background(accent, CircleShape),
                    )
                    Text(
                        text = rememberUsageTipAnnotatedString(
                            text = tip.text,
                            regularColor = regularColor,
                            boldColor = accent,
                        ),
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
    }
}

@Composable
private fun usageTipCategoryAccent(id: MobileUsageTipCategoryId): Color {
    return when (id) {
        MobileUsageTipCategoryId.CAPTURE_SHORTCUTS -> MaterialTheme.sgtColors.categoryImage
        MobileUsageTipCategoryId.PRESETS_AUTOMATION -> MaterialTheme.sgtColors.statusWarning
        MobileUsageTipCategoryId.RESULTS_RECOVERY -> MaterialTheme.sgtColors.statusSuccess
        MobileUsageTipCategoryId.MODELS_SEARCH -> MaterialTheme.sgtColors.categoryTextInput
        MobileUsageTipCategoryId.CREATIVE_TOOLS -> MaterialTheme.sgtColors.categoryTextSelect
    }
}

@DrawableRes
private fun usageTipCategoryIcon(id: MobileUsageTipCategoryId): Int {
    return when (id) {
        MobileUsageTipCategoryId.CAPTURE_SHORTCUTS -> R.drawable.ms_photo_camera
        MobileUsageTipCategoryId.PRESETS_AUTOMATION -> R.drawable.ms_auto_awesome
        MobileUsageTipCategoryId.RESULTS_RECOVERY -> R.drawable.ms_history
        MobileUsageTipCategoryId.MODELS_SEARCH -> R.drawable.ms_search
        MobileUsageTipCategoryId.CREATIVE_TOOLS -> R.drawable.ms_music_note
    }
}

@Composable
private fun rememberUsageTipAnnotatedString(
    text: String,
    regularColor: androidx.compose.ui.graphics.Color,
    boldColor: androidx.compose.ui.graphics.Color,
): AnnotatedString = remember(text, regularColor, boldColor) {
    buildAnnotatedString {
        var start = 0
        var isBold = false
        while (start < text.length) {
            val markerIndex = text.indexOf("**", startIndex = start)
            if (markerIndex < 0) {
                appendSegment(
                    segment = text.substring(start),
                    isBold = isBold,
                    regularColor = regularColor,
                    boldColor = boldColor,
                )
                break
            }
            if (markerIndex > start) {
                appendSegment(
                    segment = text.substring(start, markerIndex),
                    isBold = isBold,
                    regularColor = regularColor,
                    boldColor = boldColor,
                )
            }
            isBold = !isBold
            start = markerIndex + 2
        }
    }
}

private fun AnnotatedString.Builder.appendSegment(
    segment: String,
    isBold: Boolean,
    regularColor: androidx.compose.ui.graphics.Color,
    boldColor: androidx.compose.ui.graphics.Color,
) {
    if (segment.isEmpty()) {
        return
    }
    pushStyle(
        SpanStyle(
            color = if (isBold) boldColor else regularColor,
            fontWeight = if (isBold) FontWeight.SemiBold else FontWeight.Normal,
        ),
    )
    append(segment)
    pop()
}

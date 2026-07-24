@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
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
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors

@Composable
internal fun UsageTipsCard(
    locale: MobileLocaleText,
    modifier: Modifier = Modifier,
) {
    var showDialog by rememberSaveable { mutableStateOf(false) }
    val tips = locale.usageTipsList
    val accent = MaterialTheme.sgtColors.appSlotAmber

    ExpressiveSettingsCard(
        accent = accent,
        modifier = modifier
            .fillMaxWidth()
            .clickable(enabled = tips.isNotEmpty()) { showDialog = true },
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

    if (showDialog) {
        UsageTipsDialog(
            locale = locale,
            onDismiss = { showDialog = false },
        )
    }
}

@Composable
private fun UsageTipsDialog(
    locale: MobileLocaleText,
    onDismiss: () -> Unit,
) {
    ExpressiveDialogSurface(
        title = locale.usageTipsTitle,
        icon = R.drawable.ms_lightbulb,
        accent = MaterialTheme.colorScheme.primary,
        morphPair = ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Cookie6Sided),
        onDismiss = onDismiss,
        maxWidth = 560.dp,
        maxHeight = 620.dp,
        heightFraction = 0.76f,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            locale.usageTipsList.forEachIndexed { index, tip ->
                UsageTipListCard(
                    tipNumber = index + 1,
                    text = tip,
                    accent = if (index % 2 == 0) {
                        MaterialTheme.colorScheme.primary
                    } else {
                        MaterialTheme.colorScheme.secondary
                    },
                )
            }
        }
    }
}

@Composable
private fun UsageTipListCard(
    tipNumber: Int,
    text: String,
    accent: androidx.compose.ui.graphics.Color,
) {
    val regularColor = MaterialTheme.colorScheme.onSurfaceVariant
    ExpressiveDialogSectionCard(accent = accent) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.Top,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            MorphingShapeBadge(
                morphPair = ExpressiveMorphPair(MaterialShapes.Cookie4Sided, MaterialShapes.Flower),
                progress = 0.36f + ((tipNumber % 4) * 0.12f),
                containerColor = accent.copy(alpha = 0.18f),
                modifier = Modifier.size(38.dp),
            ) {
                Text(
                    text = tipNumber.toString(),
                    style = MaterialTheme.typography.labelLargeEmphasized,
                    color = accent,
                )
            }
            Text(
                text = rememberUsageTipAnnotatedString(
                    text = text,
                    regularColor = regularColor,
                    boldColor = accent,
                ),
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.weight(1f),
            )
        }
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

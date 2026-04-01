@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.annotation.DrawableRes
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.preset.ModelUsageStats
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.PresetProviderSettings
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

private data class ProviderSection(
    val name: String,
    @DrawableRes val icon: Int,
    val providerMatch: (PresetModelProvider) -> Boolean,
    val enabled: Boolean,
    val dashboardUrl: String? = null,
)

@Composable
internal fun UsageStatsDialog(
    locale: MobileLocaleText,
    providerSettings: PresetProviderSettings,
    lang: String,
    onDismiss: () -> Unit,
) {
    val allStats = ModelUsageStats.getAll()
    val catalog = PresetModelCatalog
    val uriHandler = LocalUriHandler.current
    val configuration = LocalConfiguration.current
    val isLandscape = configuration.screenWidthDp > configuration.screenHeightDp

    val sections = listOf(
        ProviderSection(
            name = "Groq",
            icon = R.drawable.ms_electric_bolt,
            providerMatch = { it == PresetModelProvider.GROQ },
            enabled = providerSettings.useGroq,
        ),
        ProviderSection(
            name = "Cerebras",
            icon = R.drawable.ms_local_fire_department,
            providerMatch = { it == PresetModelProvider.CEREBRAS },
            enabled = providerSettings.useCerebras,
            dashboardUrl = "https://cloud.cerebras.ai/",
        ),
        ProviderSection(
            name = "Google Gemini",
            icon = R.drawable.ms_auto_awesome,
            providerMatch = { it == PresetModelProvider.GOOGLE || it == PresetModelProvider.GEMINI_LIVE },
            enabled = providerSettings.useGemini,
            dashboardUrl = "https://aistudio.google.com/usage?timeRange=last-1-day&tab=rate-limit",
        ),
        ProviderSection(
            name = "OpenRouter",
            icon = R.drawable.ms_public,
            providerMatch = { it == PresetModelProvider.OPENROUTER },
            enabled = providerSettings.useOpenRouter,
            dashboardUrl = "https://openrouter.ai/activity",
        ),
        ProviderSection(
            name = "Ollama",
            icon = R.drawable.ms_terminal,
            providerMatch = { it == PresetModelProvider.OLLAMA },
            enabled = providerSettings.useOllama,
        ),
    )

    ExpressiveDialogSurface(
        title = locale.usageStatsTitle,
        icon = R.drawable.ms_auto_awesome,
        accent = MaterialTheme.colorScheme.primary,
        morphPair = ExpressiveMorphPair(MaterialShapes.Oval, MaterialShapes.Gem),
        onDismiss = onDismiss,
        supporting = null,
        maxWidth = 520.dp,
        maxHeight = if (isLandscape) 760.dp else 660.dp,
        heightFraction = if (isLandscape) 0.9f else 0.82f,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (allStats.isEmpty()) {
                ExpressiveDialogSectionCard(
                    accent = MaterialTheme.colorScheme.outline,
                ) {
                    Text(
                        text = locale.usageStatsNoData,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                }
            }

            sections.forEach { section ->
                if (!section.enabled) return@forEach
                val sectionModels = catalog.models.filter { section.providerMatch(it.provider) }
                if (sectionModels.isEmpty()) return@forEach

                val accent = usageStatsAccent(section.name)
                ExpressiveDialogSectionCard(accent = accent) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        MorphingShapeBadge(
                            morphPair = ExpressiveMorphPair(MaterialShapes.Oval, MaterialShapes.Gem),
                            progress = 0.62f,
                            containerColor = accent.copy(alpha = 0.18f),
                            modifier = Modifier.size(40.dp),
                        ) {
                            Icon(
                                painter = painterResource(section.icon),
                                contentDescription = null,
                                tint = accent,
                                modifier = Modifier.size(18.dp),
                            )
                        }
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                text = section.name,
                                style = MaterialTheme.typography.titleSmall,
                                fontWeight = FontWeight.SemiBold,
                            )
                            Text(
                                text = "${sectionModels.size} model${if (sectionModels.size == 1) "" else "s"}",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        if (section.dashboardUrl != null) {
                            val linkLabel = when (lang) {
                                "vi" -> "Xem lượng dùng ↗"
                                "ko" -> "사용량 확인 ↗"
                                else -> "Check Usage ↗"
                            }
                            ExpressiveDialogActionChip(
                                text = linkLabel,
                                accent = accent,
                                onClick = { uriHandler.openUri(section.dashboardUrl) },
                            )
                        }
                    }

                    sectionModels.forEach { model ->
                        val entry = allStats[model.fullName]
                        val isOllama = model.provider == PresetModelProvider.OLLAMA
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(top = 2.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                        ) {
                            Text(
                                text = model.localizedName(lang),
                                style = MaterialTheme.typography.bodyMedium,
                                modifier = Modifier.weight(1f),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Text(
                                text = when {
                                    isOllama -> locale.usageStatsUnlimited
                                    entry != null -> "${entry.remaining} / ${entry.total}"
                                    else -> "— / ${model.localizedQuota(lang).ifBlank { "?" }}"
                                },
                                style = MaterialTheme.typography.labelMediumEmphasized,
                                color = accent,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun usageStatsAccent(sectionName: String): Color = when (sectionName) {
    "Groq" -> MaterialTheme.colorScheme.primary
    "Cerebras" -> MaterialTheme.colorScheme.error
    "Google Gemini" -> MaterialTheme.colorScheme.tertiary
    "OpenRouter" -> MaterialTheme.colorScheme.secondary
    else -> MaterialTheme.colorScheme.onSurfaceVariant
}

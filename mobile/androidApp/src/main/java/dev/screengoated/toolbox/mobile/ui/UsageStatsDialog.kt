@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Computer
import androidx.compose.material.icons.rounded.LocalFireDepartment
import androidx.compose.material.icons.rounded.Public
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import dev.screengoated.toolbox.mobile.preset.ModelUsageStats
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.PresetProviderSettings
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

private data class ProviderSection(
    val name: String,
    val icon: ImageVector,
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

    val sections = listOf(
        ProviderSection(
            name = "Groq",
            icon = Icons.Rounded.Bolt,
            providerMatch = { it == PresetModelProvider.GROQ },
            enabled = providerSettings.useGroq,
        ),
        ProviderSection(
            name = "Cerebras",
            icon = Icons.Rounded.LocalFireDepartment,
            providerMatch = { it == PresetModelProvider.CEREBRAS },
            enabled = providerSettings.useCerebras,
            dashboardUrl = "https://cloud.cerebras.ai/",
        ),
        ProviderSection(
            name = "Google Gemini",
            icon = Icons.Rounded.AutoAwesome,
            providerMatch = { it == PresetModelProvider.GOOGLE || it == PresetModelProvider.GEMINI_LIVE },
            enabled = providerSettings.useGemini,
            dashboardUrl = "https://aistudio.google.com/usage?timeRange=last-1-day&tab=rate-limit",
        ),
        ProviderSection(
            name = "OpenRouter",
            icon = Icons.Rounded.Public,
            providerMatch = { it == PresetModelProvider.OPENROUTER },
            enabled = providerSettings.useOpenRouter,
            dashboardUrl = "https://openrouter.ai/activity",
        ),
        ProviderSection(
            name = "Ollama",
            icon = Icons.Rounded.Computer,
            providerMatch = { it == PresetModelProvider.OLLAMA },
            enabled = providerSettings.useOllama,
        ),
    )

    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Card(
            modifier = Modifier
                .fillMaxWidth(0.94f)
                .widthIn(max = 520.dp)
                .padding(16.dp),
            shape = RoundedCornerShape(20.dp),
            colors = CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.surface,
            ),
        ) {
            BoxWithConstraints(
                modifier = Modifier
                    .fillMaxWidth()
                    .fillMaxHeight(0.75f)
                    .heightIn(max = 600.dp)
                    .padding(start = 20.dp, end = 12.dp, top = 12.dp, bottom = 16.dp),
            ) {
                Column(modifier = Modifier.fillMaxWidth()) {
                    // Header
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = locale.usageStatsTitle,
                            style = MaterialTheme.typography.titleLarge,
                            fontWeight = FontWeight.SemiBold,
                        )
                        Spacer(Modifier.weight(1f))
                        IconButton(onClick = onDismiss) {
                            Icon(Icons.Rounded.Close, contentDescription = null)
                        }
                    }

                    if (allStats.isEmpty()) {
                        Text(
                            text = locale.usageStatsNoData,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(top = 16.dp, end = 8.dp),
                        )
                    }

                    // Scrollable content
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .weight(1f)
                            .verticalScroll(rememberScrollState())
                            .padding(end = 8.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Spacer(Modifier.size(4.dp))

                        for (section in sections) {
                            if (!section.enabled) continue

                            val sectionModels = catalog.models.filter { section.providerMatch(it.provider) }
                            if (sectionModels.isEmpty()) continue

                            // Provider header + dashboard link
                            val uriHandler = androidx.compose.ui.platform.LocalUriHandler.current
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(top = 8.dp),
                            ) {
                                Icon(
                                    section.icon,
                                    contentDescription = null,
                                    modifier = Modifier.size(18.dp),
                                    tint = MaterialTheme.colorScheme.primary,
                                )
                                Spacer(Modifier.size(8.dp))
                                Text(
                                    text = section.name,
                                    style = MaterialTheme.typography.titleSmall,
                                    fontWeight = FontWeight.SemiBold,
                                )
                                if (section.dashboardUrl != null) {
                                    Spacer(Modifier.weight(1f))
                                    val linkLabel = when (lang) {
                                        "vi" -> "Xem lượng dùng ↗"
                                        "ko" -> "사용량 확인 ↗"
                                        else -> "Check Usage ↗"
                                    }
                                    Text(
                                        text = linkLabel,
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.primary,
                                        modifier = Modifier
                                            .clickable { uriHandler.openUri(section.dashboardUrl) }
                                            .padding(vertical = 4.dp),
                                    )
                                }
                            }

                            // Model rows
                            for (model in sectionModels) {
                                val entry = allStats[model.fullName]
                                val isOllama = model.provider == PresetModelProvider.OLLAMA

                                Row(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .padding(vertical = 2.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Text(
                                        text = model.localizedName(lang),
                                        style = MaterialTheme.typography.bodySmall,
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
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                }
                            }

                            HorizontalDivider(
                                modifier = Modifier.padding(top = 4.dp),
                                color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.3f),
                            )
                        }
                    }
                }
            }
        }
    }
}

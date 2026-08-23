@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.preset.ModelUsageStats
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelDescriptor
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.PresetProviderSettings
import dev.screengoated.toolbox.mobile.preset.ui.providerIconRes
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.delay

internal data class ProviderSection(
    val name: String,
    val primaryProvider: PresetModelProvider,
    val providers: Set<PresetModelProvider>,
    val enabled: Boolean,
    val dashboardUrl: String? = null,
)

private data class EndpointIdentity(
    val provider: PresetModelProvider,
    val fullName: String,
)

@Composable
internal fun UsageStatsDialog(
    locale: MobileLocaleText,
    providerSettings: PresetProviderSettings,
    lang: String,
    onDismiss: () -> Unit,
) {
    val allStats by ModelUsageStats.snapshots.collectAsState()
    val nowSeconds by produceState(ModelUsageStats.nowUnixSeconds()) {
        while (true) {
            value = ModelUsageStats.nowUnixSeconds()
            delay(30_000)
        }
    }
    val uriHandler = LocalUriHandler.current
    val windowInfo = LocalWindowInfo.current
    val density = LocalDensity.current
    val windowWidth = with(density) { windowInfo.containerSize.width.toDp() }
    val windowHeight = with(density) { windowInfo.containerSize.height.toDp() }
    val isLandscape = windowWidth > windowHeight
    val endpoints = usageEndpointRepresentatives(PresetModelCatalog.models)
    val sections = usageProviderSections(providerSettings)

    ExpressiveDialogSurface(
        title = locale.usageStatsTitle,
        icon = R.drawable.ms_auto_awesome,
        accent = MaterialTheme.colorScheme.primary,
        morphPair = ExpressiveMorphPair(MaterialShapes.Oval, MaterialShapes.Gem),
        onDismiss = onDismiss,
        supporting = locale.usageStatsSessionHint,
        maxWidth = 620.dp,
        maxHeight = if (isLandscape) 760.dp else 700.dp,
        heightFraction = if (isLandscape) 0.9f else 0.86f,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (allStats.isEmpty()) {
                ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.outline) {
                    Text(
                        text = locale.usageStatsNoData,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }

            sections.forEach { section ->
                if (!section.enabled) return@forEach
                val sectionEndpoints = endpoints.filter { it.provider in section.providers }
                if (sectionEndpoints.isEmpty()) return@forEach

                val accent = usageStatsAccent(section.primaryProvider)
                ExpressiveDialogSectionCard(accent = accent) {
                    ProviderHeader(
                        section = section,
                        locale = locale,
                        accent = accent,
                        onOpenDashboard = section.dashboardUrl?.let { url ->
                            { uriHandler.openUri(url) }
                        },
                    )

                    if (section.primaryProvider == PresetModelProvider.OPENROUTER) {
                        FlowRow(
                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            val quota = sectionEndpoints
                                .first()
                                .localizedQuota(lang)
                                .ifBlank { "—" }
                            UsageMetricChip(
                                label = "${locale.usageStatsSharedQuota} · $quota",
                                accent = accent,
                            )
                            allStats[ModelUsageStats.providerKey(PresetModelProvider.OPENROUTER)]
                                ?.let { snapshot ->
                                    SnapshotChips(
                                        snapshot = snapshot,
                                        nowSeconds = nowSeconds,
                                        locale = locale,
                                        accent = accent,
                                    )
                                }
                        }
                    }

                    sectionEndpoints.forEach { model ->
                        EndpointRow(
                            model = model,
                            snapshot = allStats[
                                ModelUsageStats.endpointKey(model.provider, model.fullName)
                            ],
                            providerQuotaIsShared =
                                section.primaryProvider == PresetModelProvider.OPENROUTER,
                            nowSeconds = nowSeconds,
                            locale = locale,
                            lang = lang,
                            accent = accent,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ProviderHeader(
    section: ProviderSection,
    locale: MobileLocaleText,
    accent: Color,
    onOpenDashboard: (() -> Unit)?,
) {
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
                painter = painterResource(providerIconRes(section.primaryProvider)),
                contentDescription = null,
                tint = accent,
                modifier = Modifier.size(18.dp),
            )
        }
        Text(
            text = section.name,
            modifier = Modifier.weight(1f),
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.SemiBold,
        )
        if (onOpenDashboard != null) {
            ExpressiveDialogActionChip(
                text = locale.usageStatsCheckUsage,
                accent = accent,
                onClick = onOpenDashboard,
            )
        }
    }
}

@Composable
private fun EndpointRow(
    model: PresetModelDescriptor,
    snapshot: ModelUsageStats.UsageSnapshot?,
    providerQuotaIsShared: Boolean,
    nowSeconds: Long,
    locale: MobileLocaleText,
    lang: String,
    accent: Color,
) {
    val identityText = buildAnnotatedString {
        withStyle(SpanStyle(fontWeight = FontWeight.Medium)) {
            append(model.localizedName(lang))
        }
        withStyle(SpanStyle(color = MaterialTheme.colorScheme.onSurfaceVariant)) {
            append(" · ")
        }
        withStyle(
            SpanStyle(
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                fontFamily = FontFamily.Monospace,
                fontSize = MaterialTheme.typography.labelMedium.fontSize,
            ),
        ) {
            append(model.fullName)
        }
    }
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerHighest.copy(alpha = 0.55f),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 10.dp, vertical = 9.dp),
            verticalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(7.dp),
            ) {
                ModelPerformancePrefix(model)
                Icon(
                    painter = painterResource(providerIconRes(model.provider)),
                    contentDescription = null,
                    tint = accent,
                    modifier = Modifier.size(16.dp),
                )
                Text(
                    text = identityText,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.weight(1f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }

            val staticQuota = model.localizedQuota(lang).takeIf { it.isNotBlank() }
            if (snapshot != null) {
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    SnapshotChips(
                        snapshot = snapshot,
                        nowSeconds = nowSeconds,
                        locale = locale,
                        accent = accent,
                    )
                }
            } else if (!providerQuotaIsShared && staticQuota != null) {
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    UsageMetricChip(staticQuota, accent)
                }
            }
        }
    }
}

@Composable
private fun SnapshotChips(
    snapshot: ModelUsageStats.UsageSnapshot,
    nowSeconds: Long,
    locale: MobileLocaleText,
    accent: Color,
) {
    snapshot.metrics.forEach { metric ->
        val remaining = metric.remaining ?: "—"
        val limit = metric.limit ?: "—"
        val reset = metric.reset?.let { " · ${locale.usageStatsReset} $it" }.orEmpty()
        UsageMetricChip("${metric.kind.label} $remaining/$limit$reset", accent)
    }

    val ageSeconds = (nowSeconds - snapshot.observedAtUnixSeconds).coerceAtLeast(0)
    val minutes = (ageSeconds + 59) / 60
    val freshness = ModelUsageStats.freshnessAt(snapshot.observedAtUnixSeconds, nowSeconds)
    val (label, color) = when (freshness) {
        ModelUsageStats.Freshness.FRESH ->
            locale.usageStatsUpdatedNow to MaterialTheme.colorScheme.primary
        ModelUsageStats.Freshness.AGING ->
            "$minutes ${locale.usageStatsMinutesAgo}" to MaterialTheme.colorScheme.tertiary
        ModelUsageStats.Freshness.STALE ->
            "${locale.usageStatsStale} · $minutes ${locale.usageStatsMinutesAgo}" to
                MaterialTheme.colorScheme.error
    }
    UsageMetricChip(label, color)
}

@Composable
private fun UsageMetricChip(label: String, accent: Color) {
    Surface(
        shape = MaterialTheme.shapes.small,
        color = accent.copy(alpha = 0.12f),
        contentColor = accent,
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.labelMedium,
            modifier = Modifier.padding(horizontal = 8.dp, vertical = 5.dp),
        )
    }
}

internal fun usageEndpointRepresentatives(
    models: List<PresetModelDescriptor>,
): List<PresetModelDescriptor> =
    models
        .filter { ModelUsageStats.providerHasUsageStatistics(it.provider) }
        .groupBy { EndpointIdentity(it.provider, it.fullName) }
        .values
        .map { roles ->
            roles.minWith(
                compareBy<PresetModelDescriptor>(
                    { it.typicalLatencyMs ?: Int.MAX_VALUE },
                    PresetModelDescriptor::id,
                ),
            )
        }
        .sortedWith(
            compareBy<PresetModelDescriptor>(
                { it.typicalLatencyMs ?: Int.MAX_VALUE },
                PresetModelDescriptor::id,
            ),
        )

internal fun usageProviderSections(settings: PresetProviderSettings): List<ProviderSection> =
    listOf(
        ProviderSection(
            "Groq",
            PresetModelProvider.GROQ,
            setOf(PresetModelProvider.GROQ),
            settings.useGroq,
            "https://console.groq.com/docs/rate-limits",
        ),
        ProviderSection(
            "Gemini",
            PresetModelProvider.GOOGLE,
            setOf(PresetModelProvider.GOOGLE, PresetModelProvider.GEMINI_LIVE),
            settings.useGemini,
            "https://aistudio.google.com/usage?timeRange=last-1-day&tab=rate-limit",
        ),
        ProviderSection(
            "OpenRouter",
            PresetModelProvider.OPENROUTER,
            setOf(PresetModelProvider.OPENROUTER),
            settings.useOpenRouter,
            "https://openrouter.ai/activity",
        ),
        ProviderSection(
            "NVIDIA",
            PresetModelProvider.NVIDIA,
            setOf(PresetModelProvider.NVIDIA),
            settings.useNvidia,
            "https://build.nvidia.com/settings/api-keys",
        ),
        providerSection("Taalas", PresetModelProvider.TAALAS),
        providerSection("Google Translate", PresetModelProvider.GOOGLE_GTX),
        providerSection("QR", PresetModelProvider.QRSERVER),
    )

private fun providerSection(name: String, provider: PresetModelProvider): ProviderSection =
    ProviderSection(name, provider, setOf(provider), enabled = true)

@Composable
private fun usageStatsAccent(provider: PresetModelProvider): Color = when (provider) {
    PresetModelProvider.GROQ -> MaterialTheme.colorScheme.primary
    PresetModelProvider.GOOGLE,
    PresetModelProvider.GEMINI_LIVE,
    -> MaterialTheme.colorScheme.tertiary
    PresetModelProvider.OPENROUTER -> MaterialTheme.colorScheme.secondary
    PresetModelProvider.NVIDIA -> MaterialTheme.colorScheme.primary
    PresetModelProvider.TAALAS -> MaterialTheme.colorScheme.primary
    else -> MaterialTheme.colorScheme.onSurfaceVariant
}

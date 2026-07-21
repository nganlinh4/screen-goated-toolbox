package dev.screengoated.toolbox.mobile.ui

import androidx.compose.runtime.Immutable
import dev.screengoated.toolbox.mobile.history.HistoryUiState

@Immutable
internal data class ShellSectionRequest(
    val section: MobileShellSection,
    val serial: Long,
)

/**
 * Cohesive UI-state/handler holders for the mobile shell.
 *
 * These collapse what used to be ~45 individually prop-drilled parameters through
 * [SgtMobileApp] -> [MobileShellSurface] -> [SectionDetail] into a handful of
 * `@Immutable` groups organised by concern. Each holder is stable, so passing it
 * down does not defeat Compose skipping, and the grouping keeps previews/tests
 * able to construct just the slice they exercise.
 */

/** Provider API keys and endpoint config, plus their change handlers. */
@Immutable
data class ProviderKeysState(
    val apiKey: String,
    val cerebrasApiKey: String,
    val groqApiKey: String,
    val openRouterApiKey: String,
    val ollamaUrl: String,
    val onApiKeyChanged: (String) -> Unit,
    val onCerebrasApiKeyChanged: (String) -> Unit,
    val onGroqApiKeyChanged: (String) -> Unit,
    val onOpenRouterApiKeyChanged: (String) -> Unit,
    val onOllamaUrlChanged: (String) -> Unit,
)

/** History list data, search query, and the handlers driving the History section. */
@Immutable
data class HistoryUiBundle(
    val state: HistoryUiState,
    val searchQuery: String,
    val onSearchQueryChanged: (String) -> Unit,
    val onClearSearchQuery: () -> Unit,
    val onMaxItemsChanged: (Int) -> Unit,
    val onDeleteItem: (Long) -> Unit,
    val onClearItems: () -> Unit,
)

/** Click handlers for the Settings section (dialogs, reset, overlay opacity, updates). */
@Immutable
internal data class SettingsActions(
    val onPresetRuntimeSettingsClick: () -> Unit,
    val onCustomModelsClick: () -> Unit,
    val onUsageStatsClick: () -> Unit,
    val onDownloadedToolsClick: () -> Unit,
    val onResetDefaults: () -> Unit,
    val onVoiceSettingsClick: () -> Unit,
    val onOverlayOpacityChanged: (Int) -> Unit,
    val onCheckForAppUpdates: () -> Unit,
)

/** Navigation/session handlers that open shell overlays or toggle the live session. */
@Immutable
internal data class ShellNavActions(
    val onSessionToggle: () -> Unit,
    val onDownloaderClick: () -> Unit,
    val onDjClick: () -> Unit,
    val onTranslationGummyClick: () -> Unit,
    val onImageTo3dClick: () -> Unit,
    val onImageToSvgClick: () -> Unit,
    val onPresetClick: (String) -> Unit,
)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.window.PopupProperties
import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import androidx.compose.ui.unit.dp

@Composable
fun RealtimeOverlaySurface(
    state: RealtimeOverlayUiState,
    languages: List<String>,
    onTargetLanguageSelected: (String) -> Unit,
    onCopyTranscript: () -> Unit,
    onCopyTranslation: () -> Unit,
    onIncreaseFont: () -> Unit,
    onDecreaseFont: () -> Unit,
    onToggleListeningVisibility: () -> Unit,
    onToggleTranslationVisibility: () -> Unit,
    onToggleListeningHeader: () -> Unit,
    onToggleTranslationHeader: () -> Unit,
    onWindowDrag: (Int, Int) -> Unit,
    onWindowResize: (Int, Int) -> Unit,
) {
    val panelsVisible = listOf(state.listeningVisible, state.translationVisible).count { it }
    if (panelsVisible == 0) {
        return
    }

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        val stacked = maxHeight > maxWidth
        val splitGap = 12.dp
        val paneModifier = when {
            panelsVisible == 1 -> Modifier.fillMaxSize()
            stacked -> Modifier
                .fillMaxWidth()
                .height(((maxHeight - splitGap) / 2).coerceAtLeast(110.dp))
            else -> Modifier
                .width(((maxWidth - splitGap) / 2).coerceAtLeast(180.dp))
                .fillMaxSize()
        }
        Box(modifier = Modifier.fillMaxSize()) {
            if (stacked) {
                Column(
                    modifier = Modifier.fillMaxSize(),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    OverlayPanels(
                        paneModifier = paneModifier,
                        state = state,
                        languages = languages,
                        onTargetLanguageSelected = onTargetLanguageSelected,
                        onCopyTranscript = onCopyTranscript,
                        onCopyTranslation = onCopyTranslation,
                        onIncreaseFont = onIncreaseFont,
                        onDecreaseFont = onDecreaseFont,
                        onToggleListeningVisibility = onToggleListeningVisibility,
                        onToggleTranslationVisibility = onToggleTranslationVisibility,
                        onToggleListeningHeader = onToggleListeningHeader,
                        onToggleTranslationHeader = onToggleTranslationHeader,
                        onWindowDrag = onWindowDrag,
                    )
                }
            } else {
                Row(
                    modifier = Modifier.fillMaxSize(),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    OverlayPanels(
                        paneModifier = paneModifier,
                        state = state,
                        languages = languages,
                        onTargetLanguageSelected = onTargetLanguageSelected,
                        onCopyTranscript = onCopyTranscript,
                        onCopyTranslation = onCopyTranslation,
                        onIncreaseFont = onIncreaseFont,
                        onDecreaseFont = onDecreaseFont,
                        onToggleListeningVisibility = onToggleListeningVisibility,
                        onToggleTranslationVisibility = onToggleTranslationVisibility,
                        onToggleListeningHeader = onToggleListeningHeader,
                        onToggleTranslationHeader = onToggleTranslationHeader,
                        onWindowDrag = onWindowDrag,
                    )
                }
            }
            OverlayResizeHandle(
                modifier = Modifier.align(Alignment.BottomEnd),
                onWindowResize = onWindowResize,
            )
        }
    }
}

@Composable
private fun OverlayPanels(
    paneModifier: Modifier,
    state: RealtimeOverlayUiState,
    languages: List<String>,
    onTargetLanguageSelected: (String) -> Unit,
    onCopyTranscript: () -> Unit,
    onCopyTranslation: () -> Unit,
    onIncreaseFont: () -> Unit,
    onDecreaseFont: () -> Unit,
    onToggleListeningVisibility: () -> Unit,
    onToggleTranslationVisibility: () -> Unit,
    onToggleListeningHeader: () -> Unit,
    onToggleTranslationHeader: () -> Unit,
    onWindowDrag: (Int, Int) -> Unit,
) {
    val sgtColors = MaterialTheme.sgtColors
    if (state.listeningVisible) {
        OverlayPane(
            modifier = paneModifier,
            accentColor = sgtColors.overlayListeningAccent,
            title = { ListeningTitle() },
            headerCollapsed = state.listeningHeaderCollapsed,
            onHeaderToggle = onToggleListeningHeader,
            onWindowDrag = onWindowDrag,
            controls = {
                OverlayIconBadge(
                    icon = if (state.sourceMode == SourceMode.MIC) {
                        R.drawable.ms_mic
                    } else {
                        R.drawable.ms_surround_sound
                    },
                    tint = sgtColors.overlaySubtitleActiveTint,
                )
                OverlayIconBadge(icon = R.drawable.ms_auto_awesome, tint = sgtColors.overlaySubtitleActiveTint)
                OverlayLanguageChip(
                    label = "EN",
                    enabled = false,
                    languages = emptyList(),
                    selectedLanguage = "",
                    onTargetLanguageSelected = {},
                )
                OverlayActionButton(
                    icon = R.drawable.ms_content_copy,
                    tint = sgtColors.overlayIconTint,
                    onClick = onCopyTranscript,
                )
                OverlayActionButton(
                    icon = R.drawable.ms_remove,
                    tint = sgtColors.overlayIconTint,
                    onClick = onDecreaseFont,
                )
                OverlayActionButton(
                    icon = R.drawable.ms_add,
                    tint = sgtColors.overlayIconTint,
                    onClick = onIncreaseFont,
                )
                OverlayVisibilityButton(
                    icon = R.drawable.ms_subtitles,
                    tint = sgtColors.overlaySubtitleActiveTint,
                    active = state.listeningVisible,
                    onClick = onToggleListeningVisibility,
                )
                OverlayVisibilityButton(
                    icon = R.drawable.ms_translate,
                    tint = sgtColors.overlayTranslateActiveTint,
                    active = state.translationVisible,
                    onClick = onToggleTranslationVisibility,
                )
            },
        ) {
            OverlayTextBody(
                text = state.transcript,
                placeholder = "Waiting for speech...",
                fontSizeSp = state.fontSizeSp,
            )
        }
    }

    if (state.translationVisible) {
        OverlayPane(
            modifier = paneModifier,
            accentColor = sgtColors.overlayTranslationAccent,
            title = {
                Text(
                    text = "Translation",
                    color = sgtColors.overlayTranslationTitle,
                    style = MaterialTheme.typography.titleSmall,
                )
            },
            headerCollapsed = state.translationHeaderCollapsed,
            onHeaderToggle = onToggleTranslationHeader,
            onWindowDrag = onWindowDrag,
            controls = {
                OverlayActionButton(
                    icon = R.drawable.ms_volume_up,
                    tint = sgtColors.overlayIconTint,
                    enabled = false,
                    onClick = {},
                )
                OverlayIconBadge(icon = R.drawable.ms_auto_awesome, tint = sgtColors.overlayIconTint)
                OverlayLanguageChip(
                    label = LanguageCatalog.codeForName(state.targetLanguage),
                    enabled = true,
                    languages = languages,
                    selectedLanguage = state.targetLanguage,
                    onTargetLanguageSelected = onTargetLanguageSelected,
                )
                OverlayActionButton(
                    icon = R.drawable.ms_content_copy,
                    tint = sgtColors.overlayIconTint,
                    onClick = onCopyTranslation,
                )
                OverlayActionButton(
                    icon = R.drawable.ms_remove,
                    tint = sgtColors.overlayIconTint,
                    onClick = onDecreaseFont,
                )
                OverlayActionButton(
                    icon = R.drawable.ms_add,
                    tint = sgtColors.overlayIconTint,
                    onClick = onIncreaseFont,
                )
                OverlayVisibilityButton(
                    icon = R.drawable.ms_subtitles,
                    tint = sgtColors.overlaySubtitleActiveTint,
                    active = state.listeningVisible,
                    onClick = onToggleListeningVisibility,
                )
                OverlayVisibilityButton(
                    icon = R.drawable.ms_translate,
                    tint = sgtColors.overlayTranslateActiveTint,
                    active = state.translationVisible,
                    onClick = onToggleTranslationVisibility,
                )
            },
        ) {
            OverlayTranslationBody(
                committedTranslation = state.committedTranslation,
                liveTranslation = state.liveTranslation,
                placeholder = "Waiting for speech...",
                fontSizeSp = state.fontSizeSp,
            )
        }
    }
}

@Composable
private fun OverlayLanguageChip(
    label: String,
    enabled: Boolean,
    languages: List<String>,
    selectedLanguage: String,
    onTargetLanguageSelected: (String) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }

    Box {
        OverlayLanguageButton(
            label = label,
            enabled = enabled,
            onClick = {
                if (enabled && languages.isNotEmpty()) {
                    expanded = true
                }
            },
        )
        DropdownMenu(
            expanded = expanded,
            onDismissRequest = { expanded = false },
            modifier = Modifier
                .heightIn(max = 280.dp)
                .widthIn(min = 180.dp),
            properties = PopupProperties(focusable = false),
        ) {
            Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
                languages.forEach { language ->
                    DropdownMenuItem(
                        text = { Text(language) },
                        onClick = {
                            expanded = false
                            onTargetLanguageSelected(language)
                        },
                        trailingIcon = {
                            if (language == selectedLanguage) {
                                Text(LanguageCatalog.codeForName(language))
                            }
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun OverlayLanguageButton(
    label: String,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    val sgtColors = MaterialTheme.sgtColors
    androidx.compose.material3.Surface(
        color = sgtColors.overlayActionButtonBg,
        shape = androidx.compose.foundation.shape.CircleShape,
        enabled = enabled,
        onClick = onClick,
    ) {
        Text(
            text = label,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
            color = if (enabled) sgtColors.overlayActiveButtonText else sgtColors.overlayInactiveButtonText,
            style = MaterialTheme.typography.labelMedium,
        )
    }
}

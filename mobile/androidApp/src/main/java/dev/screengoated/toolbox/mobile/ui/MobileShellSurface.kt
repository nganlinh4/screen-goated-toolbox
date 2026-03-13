@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun MobileShellSurface(
    state: LiveSessionState,
    apiKey: String,
    cerebrasApiKey: String,
    globalTtsSettings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onVoiceSettingsClick: () -> Unit,
    onSessionToggle: () -> Unit,
) {
    val isActive = state.phase in setOf(
        SessionPhase.STARTING,
        SessionPhase.LISTENING,
        SessionPhase.TRANSLATING,
    )
    val canToggle = apiKey.isNotBlank() || isActive
    var selectedSection by rememberSaveable { mutableStateOf(MobileShellSection.GLOBAL) }

    BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
        val wideLayout = maxWidth >= 760.dp
        val scrollState = rememberScrollState()

        if (wideLayout) {
            Row(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(scrollState)
                    .padding(horizontal = 20.dp, vertical = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(18.dp),
            ) {
                ShellRail(
                    selectedSection = selectedSection,
                    onSectionSelected = { selectedSection = it },
                    locale = locale,
                    modifier = Modifier.fillMaxHeight(),
                )
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .widthIn(max = 960.dp),
                    verticalArrangement = Arrangement.spacedBy(18.dp),
                ) {
                    QuickActionsRow(locale = locale)
                    SectionDetail(
                        selectedSection = selectedSection,
                        state = state,
                        apiKey = apiKey,
                        cerebrasApiKey = cerebrasApiKey,
                        globalTtsSettings = globalTtsSettings,
                        locale = locale,
                        wideLayout = true,
                        onApiKeyChanged = onApiKeyChanged,
                        onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                        onVoiceSettingsClick = onVoiceSettingsClick,
                        onSessionToggle = onSessionToggle,
                        canToggle = canToggle,
                    )
                }
            }
        } else {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(scrollState)
                    .padding(horizontal = 20.dp, vertical = 12.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                SectionDeck(
                    selectedSection = selectedSection,
                    onSectionSelected = { selectedSection = it },
                    locale = locale,
                )
                QuickActionsRow(locale = locale)
                SectionDetail(
                    selectedSection = selectedSection,
                    state = state,
                    apiKey = apiKey,
                    cerebrasApiKey = cerebrasApiKey,
                    globalTtsSettings = globalTtsSettings,
                    locale = locale,
                    wideLayout = false,
                    onApiKeyChanged = onApiKeyChanged,
                    onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                    onVoiceSettingsClick = onVoiceSettingsClick,
                    onSessionToggle = onSessionToggle,
                    canToggle = canToggle,
                )
            }
        }
    }
}

internal enum class MobileShellSection {
    GLOBAL,
    HISTORY,
    PRESETS,
}

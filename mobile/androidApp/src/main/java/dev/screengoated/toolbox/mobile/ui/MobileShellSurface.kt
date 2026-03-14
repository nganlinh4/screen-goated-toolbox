@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.ui.input.pointer.pointerInput
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import kotlinx.coroutines.launch
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
    val canToggle = true
    var selectedSection by rememberSaveable { mutableStateOf(MobileShellSection.APPS) }

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
                    verticalArrangement = Arrangement.spacedBy(ShellSpacing.sectionGap),
                ) {
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
            val sections = MobileShellSection.entries
            val pagerState = rememberPagerState { sections.size }
            val scope = rememberCoroutineScope()

            // Sync pager → selectedSection
            LaunchedEffect(pagerState) {
                snapshotFlow { pagerState.currentPage }.collect { page ->
                    selectedSection = sections[page]
                }
            }
            // Sync selectedSection → pager (from button taps)
            LaunchedEffect(selectedSection) {
                val target = selectedSection.ordinal
                if (pagerState.currentPage != target) {
                    pagerState.animateScrollToPage(target)
                }
            }

            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 20.dp, vertical = 4.dp),
                verticalArrangement = Arrangement.spacedBy(ShellSpacing.sectionGap),
            ) {
                SectionSegmentedRow(
                    selectedSection = selectedSection,
                    onSectionSelected = {
                        selectedSection = it
                        scope.launch { pagerState.animateScrollToPage(it.ordinal) }
                    },
                    locale = locale,
                    modifier = Modifier.pointerInput(Unit) {
                        var totalDrag = 0f
                        detectHorizontalDragGestures(
                            onDragStart = { totalDrag = 0f },
                            onHorizontalDrag = { _, dragAmount -> totalDrag += dragAmount },
                            onDragEnd = {
                                val threshold = 80f
                                val current = pagerState.currentPage
                                val target = when {
                                    totalDrag < -threshold && current < sections.lastIndex -> current + 1
                                    totalDrag > threshold && current > 0 -> current - 1
                                    else -> null
                                }
                                if (target != null) {
                                    scope.launch { pagerState.animateScrollToPage(target) }
                                }
                            },
                        )
                    },
                )
                HorizontalPager(
                    state = pagerState,
                    modifier = Modifier
                        .fillMaxSize()
                        .weight(1f),
                    beyondViewportPageCount = 1,
                    pageSpacing = 16.dp,
                ) { page ->
                    Column(
                        modifier = Modifier
                            .fillMaxSize()
                            .verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.spacedBy(ShellSpacing.sectionGap),
                    ) {
                        SectionDetail(
                            selectedSection = sections[page],
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
    }
}

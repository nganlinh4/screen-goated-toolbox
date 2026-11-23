package dev.screengoated.toolbox.mobile.translationgummy

import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import kotlinx.coroutines.flow.Flow

@Composable
internal fun TranslationGummyMeterView(
    connectionState: TranslationGummyConnectionState,
    levels: Flow<Float>,
    accent: Color,
    modifier: Modifier = Modifier,
) {
    key(connectionState) {
        val level by levels.collectAsState(initial = 0f)
        CompactWaveform(connectionState, level, accent, modifier)
    }
}

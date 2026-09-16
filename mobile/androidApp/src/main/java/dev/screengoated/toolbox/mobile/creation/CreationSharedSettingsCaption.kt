package dev.screengoated.toolbox.mobile.creation

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

internal fun CreationNativeUiState.sharedDraftCount(): Int {
    val selected = selectedItem ?: return 0
    if (selected.submitted || selected.stage != CreationNativeStage.DRAFT) return 0
    return items.count { it.batchId == selected.batchId && !it.submitted && it.stage == CreationNativeStage.DRAFT }
}

@Composable
internal fun CreationSharedSettingsCaption(state: CreationNativeUiState, locale: MobileLocaleText) {
    val count = state.sharedDraftCount()
    if (count <= 1) return
    Text(
        text = when (locale.languageCode()) {
            "vi" -> "Áp dụng cho cả $count ảnh chưa tạo"
            "ko" -> "아직 생성하지 않은 이미지 ${count}개 모두에 적용"
            else -> "Changes apply to all $count unsubmitted images"
        },
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

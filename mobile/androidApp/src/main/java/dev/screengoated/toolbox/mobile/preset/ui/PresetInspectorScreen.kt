@file:OptIn(ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.material.icons.rounded.Star
import androidx.compose.material.icons.rounded.StarOutline
import androidx.compose.material3.AssistChip
import androidx.compose.material3.AssistChipDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.shared.preset.BlockType

@Composable
fun PresetInspectorScreen(
    resolvedPreset: ResolvedPreset,
    lang: String,
    onBack: () -> Unit,
    onFavoriteToggle: () -> Unit,
    onRestoreDefault: () -> Unit,
    onEdit: () -> Unit = {},
) {
    val preset = resolvedPreset.preset

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        text = preset.name(lang),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Rounded.ArrowBack, contentDescription = null)
                    }
                },
                actions = {
                    IconButton(onClick = onEdit) {
                        Icon(Icons.Rounded.Edit, contentDescription = null)
                    }
                    IconButton(onClick = onFavoriteToggle) {
                        Icon(
                            imageVector = if (preset.isFavorite) {
                                Icons.Rounded.Star
                            } else {
                                Icons.Rounded.StarOutline
                            },
                            contentDescription = null,
                        )
                    }
                },
            )
        },
    ) { padding ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            item {
                SummaryCard(resolvedPreset = resolvedPreset, lang = lang)
            }
            item {
                ExecutionCard(
                    resolvedPreset = resolvedPreset,
                    lang = lang,
                    onRestoreDefault = onRestoreDefault,
                )
            }
            item {
                BehaviorCard(resolvedPreset = resolvedPreset, lang = lang)
            }
            if (resolvedPreset.placeholderReasons.isNotEmpty()) {
                item {
                    PlaceholderCard(
                        reasons = resolvedPreset.placeholderReasons.toList(),
                        lang = lang,
                    )
                }
            }
            item {
                ChainSummaryCard(resolvedPreset = resolvedPreset, lang = lang)
            }
        }
    }
}

@Composable
private fun SummaryCard(
    resolvedPreset: ResolvedPreset,
    lang: String,
) {
    val preset = resolvedPreset.preset
    InfoCard(title = localized(lang, "Preset", "Preset", "프리셋")) {
        Text(
            text = preset.name(lang),
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.SemiBold,
        )
        Spacer(Modifier.height(8.dp))
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                InspectorChip(label = typeLabel(preset, lang))
                InspectorChip(label = if (resolvedPreset.isBuiltIn) {
                    localized(lang, "Built-in", "Mặc định", "기본 제공")
                } else {
                    localized(lang, "Custom", "Tự tạo", "사용자 지정")
                })
            }
            if (resolvedPreset.hasOverride || preset.isUpcoming) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (resolvedPreset.hasOverride) {
                        InspectorChip(label = localized(lang, "Override active", "Đang ghi đè", "오버라이드 적용 중"))
                    }
                    if (preset.isUpcoming) {
                        InspectorChip(label = localized(lang, "Upcoming", "Sắp ra mắt", "출시 예정"))
                    }
                }
            }
        }
        Spacer(Modifier.height(12.dp))
        InspectorRow(
            label = localized(lang, "Preset ID", "ID preset", "프리셋 ID"),
            value = preset.id,
        )
    }
}

@Composable
private fun ExecutionCard(
    resolvedPreset: ResolvedPreset,
    lang: String,
    onRestoreDefault: () -> Unit,
) {
    val preset = resolvedPreset.preset
    InfoCard(title = localized(lang, "Execution", "Thực thi", "실행")) {
        val capability = resolvedPreset.executionCapability
        Text(
            text = if (capability.supported) {
                localized(
                    lang,
                    "This preset can run on Android, but only through the SGT bubble overlay runtime.",
                    "Preset này chạy được trên Android, nhưng chỉ qua bubble overlay của SGT.",
                    "이 프리셋은 Android에서 실행할 수 있지만 SGT 버블 오버레이 런타임에서만 실행됩니다.",
                )
            } else {
                placeholderMessage(capability.reason, lang)
            },
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(12.dp))
        Text(
            text = if (preset.isFavorite) {
                localized(
                    lang,
                    "Turn on the Quick Settings bubble, tap the bubble, then launch this preset from the floating favorites panel.",
                    "Bật bubble trong Quick Settings, chạm bubble, rồi chạy preset này từ bảng yêu thích nổi.",
                    "빠른 설정에서 버블을 켜고 버블을 탭한 다음 떠 있는 즐겨찾기 패널에서 이 프리셋을 실행하세요.",
                )
            } else {
                localized(
                    lang,
                    "Star this preset first, then use the Quick Settings bubble to launch it from the floating favorites panel.",
                    "Hãy đánh dấu sao preset này trước, rồi dùng bubble trong Quick Settings để chạy nó từ bảng yêu thích nổi.",
                    "먼저 이 프리셋에 별표를 추가한 다음 빠른 설정 버블에서 떠 있는 즐겨찾기 패널로 실행하세요.",
                )
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (resolvedPreset.hasOverride) {
            Spacer(Modifier.height(12.dp))
            OutlinedButton(onClick = onRestoreDefault) {
                Text(localized(lang, "Restore default", "Khôi phục mặc định", "기본값 복원"))
            }
        }
        if (preset.autoPaste || preset.autoPasteNewline) {
            Spacer(Modifier.height(12.dp))
            Text(
                text = localized(
                    lang,
                    "Auto-paste settings are visible for parity but stay read-only until Android has a real paste runtime.",
                    "Cài đặt tự dán được hiển thị để giữ parity nhưng vẫn chỉ đọc cho đến khi Android có runtime dán thật.",
                    "자동 붙여넣기 설정은 패리티를 위해 표시되지만 Android에 실제 붙여넣기 런타임이 생길 때까지 읽기 전용입니다.",
                ),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun BehaviorCard(
    resolvedPreset: ResolvedPreset,
    lang: String,
) {
    val preset = resolvedPreset.preset
    InfoCard(title = localized(lang, "Windows fields", "Trường Windows", "Windows 필드")) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            InspectorRow(localized(lang, "Prompt mode", "Chế độ prompt", "프롬프트 모드"), preset.promptMode)
            InspectorRow(localized(lang, "Text input mode", "Chế độ nhập text", "텍스트 입력 모드"), preset.textInputMode)
            InspectorRow(localized(lang, "Audio source", "Nguồn âm thanh", "오디오 소스"), preset.audioSource)
            InspectorRow(localized(lang, "Audio processing", "Xử lý âm thanh", "오디오 처리"), preset.audioProcessingMode)
            InspectorRow(localized(lang, "Realtime window", "Cửa sổ realtime", "실시간 창"), preset.realtimeWindowMode)
            InspectorRow(localized(lang, "Video capture", "Bắt video", "비디오 캡처"), preset.videoCaptureMethod)
            InspectorRow(localized(lang, "Auto paste", "Tự dán", "자동 붙여넣기"), yesNo(preset.autoPaste, lang))
            InspectorRow(localized(lang, "Auto paste newline", "Xuống dòng trước khi dán", "붙여넣기 전 줄바꿈"), yesNo(preset.autoPasteNewline, lang))
            InspectorRow(localized(lang, "Hide recording UI", "Ẩn UI ghi âm", "녹음 UI 숨기기"), yesNo(preset.hideRecordingUi, lang))
            InspectorRow(localized(lang, "Auto stop recording", "Tự dừng ghi âm", "자동 녹음 중지"), yesNo(preset.autoStopRecording, lang))
            InspectorRow(localized(lang, "Continuous input", "Nhập liên tục", "연속 입력"), yesNo(preset.continuousInput, lang))
            InspectorRow(localized(lang, "Controller UI", "UI điều khiển", "컨트롤러 UI"), yesNo(preset.showControllerUi, lang))
            InspectorRow(localized(lang, "Favorite", "Yêu thích", "즐겨찾기"), yesNo(preset.isFavorite, lang))
            InspectorRow(localized(lang, "Hotkeys", "Phím tắt", "단축키"), preset.hotkeys.size.toString())
        }
    }
}

@Composable
private fun PlaceholderCard(
    reasons: List<PresetPlaceholderReason>,
    lang: String,
) {
    InfoCard(title = localized(lang, "Placeholders", "Placeholder", "플레이스홀더")) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            reasons.forEach { reason ->
                Card(
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
                    ),
                ) {
                    Column(modifier = Modifier.padding(12.dp)) {
                        Text(
                            text = placeholderTitle(reason, lang),
                            style = MaterialTheme.typography.titleSmall,
                            fontWeight = FontWeight.Medium,
                        )
                        Spacer(Modifier.height(4.dp))
                        Text(
                            text = placeholderMessage(reason, lang),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ChainSummaryCard(
    resolvedPreset: ResolvedPreset,
    lang: String,
) {
    val preset = resolvedPreset.preset
    InfoCard(title = localized(lang, "Processing chain", "Chuỗi xử lý", "처리 체인")) {
        if (preset.blocks.isEmpty()) {
            Text(
                text = localized(lang, "No blocks", "Không có block", "블록 없음"),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            return@InfoCard
        }

        Text(
            text = localized(
                lang,
                "Tap the edit button (top-right) to open the graph editor.",
                "Nhấn nút sửa (trên bên phải) để mở trình chỉnh sửa graph.",
                "편집 버튼(오른쪽 상단)을 눌러 그래프 편집기를 열 수 있습니다.",
            ),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(12.dp))
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            preset.blocks.forEachIndexed { index, block ->
                Card(
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
                    ),
                ) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(12.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Text(
                            text = "${localized(lang, "Block", "Block", "블록")} ${index + 1}: ${blockTypeLabel(block.blockType, lang)}",
                            style = MaterialTheme.typography.titleSmall,
                            fontWeight = FontWeight.Medium,
                        )
                        InspectorRow(localized(lang, "Model", "Model", "모델"), block.model.ifBlank { "-" })
                        InspectorRow(localized(lang, "Render mode", "Kiểu hiển thị", "렌더 모드"), block.renderMode)
                        InspectorRow(localized(lang, "Show overlay", "Hiện overlay", "오버레이 표시"), yesNo(block.showOverlay, lang))
                        InspectorRow(localized(lang, "Auto copy", "Tự copy", "자동 복사"), yesNo(block.autoCopy, lang))
                        InspectorRow(localized(lang, "Auto speak", "Tự nói", "자동 말하기"), yesNo(block.autoSpeak, lang))
                        if (block.prompt.isNotBlank()) {
                            Text(
                                text = block.prompt,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }
        }
        if (preset.blockConnections.isNotEmpty()) {
            Spacer(Modifier.height(12.dp))
            Text(
                text = preset.blockConnections.joinToString(
                    separator = "\n",
                    prefix = localized(lang, "Connections:\n", "Kết nối:\n", "연결:\n"),
                ) { (from, to) -> "${from + 1} -> ${to + 1}" },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun InfoCard(
    title: String,
    content: @Composable () -> Unit,
) {
    Card(
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainer,
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
        ) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Spacer(Modifier.height(12.dp))
            content()
        }
    }
}

@Composable
private fun InspectorRow(
    label: String,
    value: String,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            text = label,
            modifier = Modifier.weight(1f),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            text = value,
            modifier = Modifier.weight(1f),
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.Medium,
        )
    }
}

@Composable
private fun InspectorChip(label: String) {
    AssistChip(
        onClick = {},
        enabled = false,
        label = { Text(label) },
        colors = AssistChipDefaults.assistChipColors(),
    )
}

private fun typeLabel(
    preset: dev.screengoated.toolbox.mobile.shared.preset.Preset,
    lang: String,
): String = when (preset.presetType) {
    dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE ->
        localized(lang, "Image", "Ảnh", "이미지")
    dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT ->
        localized(lang, "Text select", "Bôi text", "텍스트 선택")
    dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT ->
        localized(lang, "Text input", "Nhập text", "텍스트 입력")
    dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC ->
        localized(lang, "Mic audio", "Mic", "마이크")
    dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO ->
        localized(lang, "Device audio", "Âm thanh hệ thống", "기기 오디오")
}

private fun blockTypeLabel(type: BlockType, lang: String): String = when (type) {
    BlockType.INPUT_ADAPTER -> localized(lang, "Input adapter", "Bộ nhận input", "입력 어댑터")
    BlockType.IMAGE -> localized(lang, "Image block", "Block ảnh", "이미지 블록")
    BlockType.TEXT -> localized(lang, "Text block", "Block text", "텍스트 블록")
    BlockType.AUDIO -> localized(lang, "Audio block", "Block âm thanh", "오디오 블록")
}

private fun placeholderTitle(reason: PresetPlaceholderReason, lang: String): String = when (reason) {
    PresetPlaceholderReason.IMAGE_CAPTURE_NOT_READY ->
        localized(lang, "Image capture", "Bắt ảnh", "이미지 캡처")
    PresetPlaceholderReason.TEXT_SELECTION_NOT_READY ->
        localized(lang, "Selected text capture", "Bắt text được chọn", "선택 텍스트 캡처")
    PresetPlaceholderReason.TEXT_INPUT_OVERLAY_NOT_READY ->
        localized(lang, "Overlay input", "Nhập overlay", "오버레이 입력")
    PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY ->
        localized(lang, "Audio capture", "Bắt âm thanh", "오디오 캡처")
    PresetPlaceholderReason.REALTIME_AUDIO_NOT_READY ->
        localized(lang, "Realtime audio", "Âm thanh realtime", "실시간 오디오")
    PresetPlaceholderReason.HTML_RESULT_NOT_READY ->
        localized(lang, "HTML result runtime", "Runtime kết quả HTML", "HTML 결과 런타임")
    PresetPlaceholderReason.CONTROLLER_MODE_NOT_READY ->
        localized(lang, "Controller mode", "Chế độ controller", "컨트롤러 모드")
    PresetPlaceholderReason.AUTO_PASTE_NOT_READY ->
        localized(lang, "Auto paste", "Tự dán", "자동 붙여넣기")
    PresetPlaceholderReason.HOTKEYS_NOT_READY ->
        localized(lang, "Hotkeys", "Phím tắt", "단축키")
    PresetPlaceholderReason.GRAPH_EDITING_NOT_READY ->
        localized(lang, "Graph editor", "Trình chỉnh graph", "그래프 편집기")
    PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY ->
        localized(lang, "Non-text graph runtime", "Runtime graph không phải text", "비텍스트 그래프 런타임")
}

private fun placeholderMessage(reason: PresetPlaceholderReason?, lang: String): String {
    if (reason == null) {
        return localized(lang, "Ready", "Sẵn sàng", "준비됨")
    }
    return when (reason) {
        PresetPlaceholderReason.IMAGE_CAPTURE_NOT_READY ->
            localized(lang, "Android does not have the Windows image capture runtime yet.", "Android chưa có runtime bắt ảnh như Windows.", "Android에는 아직 Windows식 이미지 캡처 런타임이 없습니다.")
        PresetPlaceholderReason.TEXT_SELECTION_NOT_READY ->
            localized(lang, "Android does not have the Windows selected-text capture flow yet.", "Android chưa có luồng bắt text được chọn như Windows.", "Android에는 아직 Windows식 선택 텍스트 캡처 흐름이 없습니다.")
        PresetPlaceholderReason.TEXT_INPUT_OVERLAY_NOT_READY ->
            localized(lang, "Overlay-style text input is still a placeholder on Android.", "Nhập text kiểu overlay vẫn chỉ là placeholder trên Android.", "오버레이식 텍스트 입력은 아직 Android에서 플레이스홀더입니다.")
        PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY ->
            localized(lang, "Android does not have the Windows record-then-process audio runtime yet.", "Android chưa có runtime ghi âm rồi xử lý như Windows.", "Android에는 아직 Windows식 녹음 후 처리 런타임이 없습니다.")
        PresetPlaceholderReason.REALTIME_AUDIO_NOT_READY ->
            localized(lang, "Realtime audio parity is not ready on Android yet.", "Parity âm thanh realtime trên Android chưa sẵn sàng.", "실시간 오디오 패리티는 아직 Android에서 준비되지 않았습니다.")
        PresetPlaceholderReason.HTML_RESULT_NOT_READY ->
            localized(lang, "This preset expects raw HTML result rendering, and Android only ships the markdown overlay runtime right now.", "Preset này cần hiển thị kết quả HTML thuần, trong khi Android hiện chỉ có runtime overlay markdown.", "이 프리셋은 원시 HTML 결과 렌더링이 필요하지만 Android는 현재 마크다운 오버레이 런타임만 제공합니다.")
        PresetPlaceholderReason.CONTROLLER_MODE_NOT_READY ->
            localized(lang, "Master/controller preset orchestration is still a placeholder on Android.", "Điều phối preset master/controller vẫn chỉ là placeholder trên Android.", "마스터/컨트롤러 프리셋 오케스트레이션은 아직 Android에서 플레이스홀더입니다.")
        PresetPlaceholderReason.AUTO_PASTE_NOT_READY ->
            localized(lang, "Auto-paste exists in the Windows model but Android cannot execute it honestly yet.", "Tự dán có trong model Windows nhưng Android chưa thể làm thật một cách trung thực.", "자동 붙여넣기는 Windows 모델에는 있지만 Android에서는 아직 정직하게 구현할 수 없습니다.")
        PresetPlaceholderReason.HOTKEYS_NOT_READY ->
            localized(lang, "Hotkeys are kept visible for parity but are not wired on Android.", "Phím tắt được giữ để parity nhưng chưa được nối trên Android.", "단축키는 패리티를 위해 표시되지만 Android에서는 아직 연결되지 않았습니다.")
        PresetPlaceholderReason.GRAPH_EDITING_NOT_READY ->
            localized(lang, "Use the edit button to open the full node graph editor.", "Dùng nút sửa để mở trình chỉnh sửa graph đầy đủ.", "편집 버튼을 사용하여 전체 노드 그래프 편집기를 열 수 있습니다.")
        PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY ->
            localized(lang, "Only text-input graphs can execute on Android right now.", "Hiện tại Android chỉ chạy được graph text-input.", "현재 Android에서는 텍스트 입력 그래프만 실행할 수 있습니다.")
    }
}

private fun yesNo(value: Boolean, lang: String): String = if (value) {
    localized(lang, "Yes", "Có", "예")
} else {
    localized(lang, "No", "Không", "아니오")
}

private fun localized(
    lang: String,
    en: String,
    vi: String,
    ko: String,
): String = when (lang) {
    "vi" -> vi
    "ko" -> ko
    else -> en
}

@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class, androidx.compose.ui.text.ExperimentalTextApi::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.annotation.DrawableRes
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.DEFAULT_IMAGE_MODEL_ID
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetType

@Composable
internal fun HeaderSection(
    editState: Preset,
    lang: String,
    isBuiltIn: Boolean,
    onNameChanged: (String) -> Unit,
    onTypeGroupChanged: (EditorTypeGroup) -> Unit,
    onControllerToggled: (Boolean) -> Unit,
) {
    // Type selector card
    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
            SectionLabel(localized(lang, "Type", "Loại hình", "유형"))

            val currentGroup = editState.presetType.editorGroup()
            val groups = EditorTypeGroup.entries

            FlowRow(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
                verticalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
            ) {
                groups.forEachIndexed { index, group ->
                    ToggleButton(
                        checked = currentGroup == group,
                        onCheckedChange = { if (it) onTypeGroupChanged(group) },
                        shapes = when (index) {
                            0 -> ButtonGroupDefaults.connectedLeadingButtonShapes()
                            groups.lastIndex -> ButtonGroupDefaults.connectedTrailingButtonShapes()
                            else -> ButtonGroupDefaults.connectedMiddleButtonShapes()
                        },
                        modifier = Modifier
                            .weight(1f)
                            .semantics { role = Role.RadioButton },
                    ) {
                        Icon(
                            painterResource(editorTypeIcon(group)),
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(4.dp))
                        Text(
                            editorTypeLabel(group, lang),
                            style = MaterialTheme.typography.labelMedium,
                            fontFamily = condensedButtonFont,
                            maxLines = 1,
                            softWrap = false,
                        )
                    }
                }
            }
        }
    }

    // Controller toggle — separate card, hidden for realtime audio
    val isRealtimeAudio = editState.presetType.editorGroup() == EditorTypeGroup.AUDIO &&
        editState.audioProcessingMode == "realtime"
    if (!isRealtimeAudio) {
        SectionCard {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                SectionLabel(localized(lang, "Controller", "Bộ điều khiển", "컨트롤러"))
                Spacer(Modifier.weight(1f))
                Switch(
                    checked = editState.showControllerUi,
                    onCheckedChange = onControllerToggled,
                )
            }
        }
    }
}

// =============================================================================
// Section 2: Mode selectors
// =============================================================================

@Composable
internal fun ModeSelectorsSection(
    editState: Preset,
    lang: String,
    controllerOn: Boolean = false,
    onUpdate: (Preset) -> Unit,
) {
    val group = editState.presetType.editorGroup()
    val hideEntireSection = group == EditorTypeGroup.IMAGE && controllerOn
    if (!hideEntireSection) {
        SectionCard {
            Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                when (group) {
                    EditorTypeGroup.IMAGE -> ImageModeSelectors(editState, lang, onUpdate)
                    EditorTypeGroup.TEXT -> TextModeSelectors(editState, lang, controllerOn, onUpdate)
                    EditorTypeGroup.AUDIO -> AudioModeSelectors(editState, lang, controllerOn, onUpdate)
                }
            }
        }
    }
}

@Composable
internal fun ImageModeSelectors(
    editState: Preset,
    lang: String,
    onUpdate: (Preset) -> Unit,
) {
    TogglePair(
        label = localized(lang, "Command", "Lệnh", "명령"),
        optionA = localized(lang, "Predefined Prompt", "Làm theo lệnh sẵn", "사전 정의된 프롬프트"),
        optionB = localized(lang, "Write on the spot", "Viết lệnh tại chỗ", "즉석에서 작성"),
        isB = editState.promptMode == "dynamic",
        onChanged = { isDynamic ->
            onUpdate(editState.copy(promptMode = if (isDynamic) "dynamic" else "fixed"))
        },
    )
}

@Composable
internal fun TextModeSelectors(
    editState: Preset,
    lang: String,
    controllerOn: Boolean = false,
    onUpdate: (Preset) -> Unit,
) {
    val isInputMode = editState.presetType == PresetType.TEXT_INPUT

    TogglePair(
        label = localized(lang, "Mode", "Phương thức", "작동 방식"),
        optionA = localized(lang, "Select text", "Bôi text", "텍스트 선택"),
        optionB = localized(lang, "Type", "Gõ text", "입력"),
        isB = isInputMode,
        onChanged = { isType ->
            val newType = if (isType) PresetType.TEXT_INPUT else PresetType.TEXT_SELECT
            onUpdate(editState.copy(presetType = newType))
        },
    )

    if (!controllerOn) {
        AnimatedVisibility(visible = isInputMode, enter = fadeIn() + expandVertically(), exit = fadeOut() + shrinkVertically()) {
            SwitchRow(
                label = localized(lang, "Continuous input", "Nhập liên tục", "연속 입력"),
                checked = editState.continuousInput,
                onCheckedChange = { onUpdate(editState.copy(continuousInput = it)) },
            )
        }
        AnimatedVisibility(visible = !isInputMode, enter = fadeIn() + expandVertically(), exit = fadeOut() + shrinkVertically()) {
            TogglePair(
                label = localized(lang, "Command", "Lệnh", "명령"),
                optionA = localized(lang, "Predefined Prompt", "Làm theo lệnh sẵn", "사전 정의된 프롬프트"),
                optionB = localized(lang, "Write on the spot", "Viết lệnh tại chỗ", "즉석에서 작성"),
                isB = editState.promptMode == "dynamic",
                onChanged = { isDynamic -> onUpdate(editState.copy(promptMode = if (isDynamic) "dynamic" else "fixed")) },
            )
        }
    }
}

@Composable
internal fun AudioModeSelectors(
    editState: Preset,
    lang: String,
    controllerOn: Boolean = false,
    onUpdate: (Preset) -> Unit,
) {
    val isRealtime = editState.audioProcessingMode == "realtime"

    // Audio source — hidden for realtime (always device audio)
    if (!isRealtime) {
        val isMic = editState.presetType == PresetType.MIC || editState.audioSource == "mic"
        TogglePair(
            label = localized(lang, "Audio Source", "Nguồn", "오디오 소스"),
            optionA = localized(lang, "Microphone", "Microphone", "마이크"),
            optionB = localized(lang, "Device Audio", "Âm thanh máy tính", "컴퓨터 오디오"),
            isB = !isMic,
            iconA = R.drawable.ms_mic,
            iconB = R.drawable.ms_speaker_phone,
            onChanged = { isDevice ->
                val newType = if (isDevice) PresetType.DEVICE_AUDIO else PresetType.MIC
                val newSource = if (isDevice) "device" else "mic"
                onUpdate(editState.copy(presetType = newType, audioSource = newSource))
            },
        )
    }

    if (controllerOn) return

    // Processing mode
    TogglePair(
        label = localized(lang, "Mode", "Phương thức", "작동 방식"),
        optionA = localized(lang, "Record then Process", "Thu âm rồi xử lý", "녹음 후 처리"),
        optionB = localized(lang, "Realtime Processing", "Xử lý thời gian thực", "실시간 처리"),
        isB = isRealtime,
        onChanged = { isRealtimeMode ->
            if (isRealtimeMode) {
                onUpdate(editState.copy(
                    presetType = PresetType.DEVICE_AUDIO,
                    audioSource = "device",
                    audioProcessingMode = "realtime",
                ))
            } else {
                onUpdate(editState.copy(audioProcessingMode = "record_then_process"))
            }
        },
    )

    // Auto-stop and other options hidden for realtime
    if (isRealtime) return

    SwitchRow(
        label = localized(lang, "Auto-stop recording", "Tự động dừng ghi", "자동 녹음 중지"),
        checked = editState.autoStopRecording,
        onCheckedChange = { onUpdate(editState.copy(autoStopRecording = it)) },
    )
}

// =============================================================================
// Section 3: Node graph
// =============================================================================

/** Convert graph state back to preset blocks + connections (BFS from input node). */

@Composable
internal fun AutoPasteSection(
    editState: Preset,
    lang: String,
    onUpdate: (Preset) -> Unit,
) {
    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    painterResource(R.drawable.ms_content_paste),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Spacer(Modifier.width(8.dp))
                SectionLabel(localized(lang, "Auto-Paste", "Tự động dán", "자동 붙여넣기"))
            }

            SwitchRow(
                label = localized(lang, "Auto-paste output", "Tự động dán kết quả", "출력 자동 붙여넣기"),
                checked = editState.autoPaste,
                onCheckedChange = { onUpdate(editState.copy(autoPaste = it)) },
            )

            AnimatedVisibility(
                visible = editState.autoPaste,
                enter = fadeIn() + expandVertically(),
                exit = fadeOut() + shrinkVertically(),
            ) {
                SwitchRow(
                    label = localized(lang, "Append newline", "Thêm dòng mới", "줄 바꿈 추가"),
                    checked = editState.autoPasteNewline,
                    onCheckedChange = { onUpdate(editState.copy(autoPasteNewline = it)) },
                )
            }
        }
    }
}

// =============================================================================
// Section 5: Master preset description
// =============================================================================

@Composable
internal fun MasterDescriptionSection(lang: String) {
    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    painterResource(R.drawable.ms_description),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Spacer(Modifier.width(8.dp))
                SectionLabel(
                    localized(lang, "Controller", "Bộ điều khiển", "컨트롤러"),
                )
            }

            Text(
                localized(
                    lang,
                    "This is a master preset that controls the execution flow of other presets. " +
                        "It does not process input directly but orchestrates multiple processing chains " +
                        "to produce a combined result.",
                    "Đây là preset chính điều khiển luồng thực thi của các preset khác. " +
                        "Nó không xử lý đầu vào trực tiếp mà điều phối nhiều chuỗi xử lý " +
                        "để tạo ra kết quả kết hợp.",
                    "이것은 다른 프리셋의 실행 흐름을 제어하는 마스터 프리셋입니다. " +
                        "입력을 직접 처리하지 않고 여러 처리 체인을 조율하여 " +
                        "결합된 결과를 생성합니다.",
                ),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
internal fun RealtimeDescriptionSection(lang: String) {
    SectionCard {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    painterResource(R.drawable.ms_audio_file),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Spacer(Modifier.width(8.dp))
                SectionLabel(
                    localized(lang,
                        "Realtime Audio Processing",
                        "Xử lý âm thanh (Thời gian thực)",
                        "실시간 오디오 처리",
                    ),
                )
            }

            Text(
                localized(
                    lang,
                    "This mode provides real-time transcription and translation.\n" +
                        "Gemini API key is required, works best on audio with clear speech like podcasts!\n\n" +
                        "You can adjust font size, audio source, and translation language directly in the result window.",
                    "Chế độ này cung cấp phụ đề và dịch thuật trực tiếp theo thời gian thực.\n" +
                        "Mã API của Gemini là bắt buộc, tính năng chỉ hoạt động tốt trên âm thanh có lời nói to rõ như podcast!\n\n" +
                        "Bạn có thể điều chỉnh cỡ chữ, nguồn âm thanh và ngôn ngữ dịch ngay trong cửa sổ kết quả.",
                    "이 모드는 실시간 자막 및 번역을 제공합니다.\n" +
                        "Gemini API 키가 필수이며, 명확한 음성이 있는 팟캐스트 같은 오디오에서 잘 작동합니다!\n\n" +
                        "결과 창에서 글꼴 크기, 오디오 소스, 번역 언어를 직접 조정할 수 있습니다.",
                ),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

// =============================================================================
// Reusable components
// =============================================================================


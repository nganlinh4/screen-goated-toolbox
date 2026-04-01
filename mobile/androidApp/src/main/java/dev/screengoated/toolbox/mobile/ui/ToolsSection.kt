@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.annotation.DrawableRes
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FloatingActionButtonMenu
import androidx.compose.material3.FloatingActionButtonMenuItem
import androidx.compose.material3.FloatingToolbarDefaults
import androidx.compose.material3.HorizontalFloatingToolbar
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asComposePath
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.dp
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.toPath
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.theme.SgtExtendedColors
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors
import kotlin.math.max
import kotlin.math.min

internal data class ToolPresetItem(
    val id: String,
    val nameEn: String,
    val nameVi: String,
    val nameKo: String,
    @DrawableRes val icon: Int,
    /** If true, `id` is the full preset ID. If false, needs "preset_" prefix. */
    val isFullId: Boolean = false,
) {
    fun name(lang: String): String = when (lang) {
        "vi" -> nameVi
        "ko" -> nameKo
        else -> nameEn
    }

    /** Split the label into two balanced lines at the best word boundary. */
    fun balancedName(lang: String): String {
        val raw = name(lang)
        val words = raw.split(" ")
        if (words.size <= 1) return "$raw\n " // pad single-word to keep 2-line height
        var bestIdx = 1
        var bestMax = Int.MAX_VALUE
        for (i in 1 until words.size) {
            val top = words.subList(0, i).joinToString(" ")
            val bot = words.subList(i, words.size).joinToString(" ")
            val m = maxOf(top.length, bot.length)
            if (m < bestMax) { bestMax = m; bestIdx = i }
        }
        val top = words.subList(0, bestIdx).joinToString(" ")
        val bot = words.subList(bestIdx, words.size).joinToString(" ")
        return "$top\n$bot"
    }
}

private data class ToolCategory(
    val labelGetter: (MobileLocaleText) -> String,
    val accentColorToken: (SgtExtendedColors) -> Color,
    val presets: List<ToolPresetItem>,
    /** Preset types that belong to this category (for routing custom presets). */
    val acceptsTypes: Set<dev.screengoated.toolbox.mobile.shared.preset.PresetType> = emptySet(),
)

private val toolCategories = listOf(
    // Column 1: Image presets (matches Windows order exactly)
    ToolCategory(
        labelGetter = { it.toolsCategoryImage },
        accentColorToken = { it.statusProcessing },
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE),
        presets = listOf(
            ToolPresetItem("translate", "Translate region", "Dịch vùng", "영역 번역", R.drawable.ms_translate),
            ToolPresetItem("extract_retranslate", "Trans (ACCURATE)", "Dịch vùng (CHUẨN)", "영역 번역 (정확)", R.drawable.ms_verified),
            ToolPresetItem("translate_auto_paste", "Trans (Auto paste)", "Dịch vùng (Tự dán)", "영역 번역 (자동 붙.)", R.drawable.ms_content_paste_go),
            ToolPresetItem("extract_table", "Extract Table", "Trích bảng", "표 추출", R.drawable.ms_table_chart),
            ToolPresetItem("translate_retranslate", "Trans+Retrans", "Dịch vùng+Dịch lại", "번역+재번역", R.drawable.ms_translate),
            ToolPresetItem("extract_retrans_retrans", "Trans (ACC)+Retrans", "D.vùng (CHUẨN)+D.lại", "번역(정확)+재번역", R.drawable.ms_verified),
            ToolPresetItem("ocr", "Extract text", "Lấy text từ ảnh", "텍스트 추출", R.drawable.ms_text_fields),
            ToolPresetItem("ocr_read", "Read this region", "Đọc vùng này", "영역 읽기", R.drawable.ms_volume_up),
            ToolPresetItem("quick_screenshot", "Quick Screenshot", "Chụp MH nhanh", "빠른 스크린샷", R.drawable.ms_photo_camera),
            ToolPresetItem("qr_scanner", "QR Scanner", "Quét mã QR", "QR 스캔", R.drawable.ms_qr_code_scanner),
            ToolPresetItem("summarize", "Summarize region", "Tóm tắt vùng", "영역 요약", R.drawable.ms_summarize),
            ToolPresetItem("desc", "Describe image", "Mô tả ảnh", "이미지 설명", R.drawable.ms_description),
            ToolPresetItem("ask_image", "Ask about image", "Hỏi về ảnh", "이미지 질문", R.drawable.ms_image_search),
            ToolPresetItem("fact_check", "Fact Check", "Kiểm chứng thông tin", "정보 확인", R.drawable.ms_fact_check),
            ToolPresetItem("omniscient_god", "Omniscient God", "Thần Trí tuệ", "전지전능", R.drawable.ms_psychology),
            ToolPresetItem("hang_image", "Image Overlay", "Treo ảnh", "이미지 오버레이", R.drawable.ms_layers),
        ),
    ),
    // Column 2a: Text Select presets
    ToolCategory(
        labelGetter = { it.toolsCategoryTextSelect },
        accentColorToken = { it.statusSuccess },
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT),
        presets = listOf(
            ToolPresetItem("read_aloud", "Read aloud", "Đọc to", "크게 읽기", R.drawable.ms_record_voice_over),
            ToolPresetItem("translate_select", "Translate", "Dịch", "번역", R.drawable.ms_g_translate),
            ToolPresetItem("translate_arena", "Trans (Arena)", "Dịch (Arena)", "번역 (아레나)", R.drawable.ms_translate),
            ToolPresetItem("trans_retrans_select", "Trans+Retrans", "Dịch+Dịch lại", "번역+재번역", R.drawable.ms_translate),
            ToolPresetItem("select_translate_replace", "Trans & Replace", "Dịch và Thay", "번역 후 교체", R.drawable.ms_find_replace),
            ToolPresetItem("fix_grammar", "Fix Grammar", "Sửa ngữ pháp", "문법 수정", R.drawable.ms_spellcheck),
            ToolPresetItem("rephrase", "Rephrase", "Viết lại", "다시 쓰기", R.drawable.ms_wand_stars),
            ToolPresetItem("make_formal", "Make Formal", "Chuyên nghiệp hóa", "공식적으로", R.drawable.ms_workspace_premium),
            ToolPresetItem("explain", "Explain", "Giải thích", "설명", R.drawable.ms_lightbulb),
            ToolPresetItem("ask_text", "Ask about text...", "Hỏi về text...", "텍스트 질문", R.drawable.ms_chat),
            ToolPresetItem("edit_as_follows", "Edit as follows:", "Sửa như sau:", "다음과 같이 수정:", R.drawable.ms_edit),
            ToolPresetItem("101_on_this", "101 on this", "Tất tần tật", "이것의 모든 것", R.drawable.ms_school),
            ToolPresetItem("hang_text", "Text Overlay", "Treo text", "텍스트 오버레이", R.drawable.ms_sticky_note_2),
        ),
    ),
    // Column 2b: Text Input (Type) presets
    ToolCategory(
        labelGetter = { it.toolsCategoryTextInput },
        accentColorToken = { it.statusSuccess },
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT),
        presets = listOf(
            ToolPresetItem("trans_retrans_typing", "Trans+Retrans (Type)", "Dịch+Dịch lại (Tự gõ)", "번역+재번역 (입력)", R.drawable.ms_translate),
            ToolPresetItem("ask_ai", "Ask AI", "Hỏi AI", "AI 질문", R.drawable.ms_cognition_2),
            ToolPresetItem("internet_search", "Internet Search", "Tìm kiếm internet", "인터넷 검색", R.drawable.ms_travel_explore),
            ToolPresetItem("make_game", "Make a Game", "Tạo con game", "게임 만들기", R.drawable.ms_gamepad),
            ToolPresetItem("quick_note", "Quick Note", "Note nhanh", "빠른 메모", R.drawable.ms_edit_note),
        ),
    ),
    // Column 3a: Mic presets
    ToolCategory(
        labelGetter = { it.toolsCategoryMicRecording },
        accentColorToken = { it.statusWarning },
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC),
        presets = listOf(
            ToolPresetItem("transcribe", "Transcribe speech", "Lời nói thành văn", "음성 받아쓰기", R.drawable.ms_mic),
            ToolPresetItem("continuous_writing_online", "Continuous Writing", "Viết liên tục", "연속 입력", R.drawable.ms_keyboard),
            ToolPresetItem("fix_pronunciation", "Fix pronunciation", "Chỉnh phát âm", "발음 교정", R.drawable.ms_record_voice_over),
            ToolPresetItem("transcribe_retranslate", "Quick 4NR reply 1", "Trả lời ng.nc.ngoài 1", "빠른 외국인 답변 1", R.drawable.ms_translate),
            ToolPresetItem("quicker_foreigner_reply", "Quick 4NR reply 2", "Trả lời ng.nc.ngoài 2", "빠른 외국인 답변 2", R.drawable.ms_translate),
            ToolPresetItem("quick_ai_question", "Quick AI Question", "Hỏi nhanh AI", "빠른 AI 질문", R.drawable.ms_quick_phrases),
            ToolPresetItem("voice_search", "Voice Search", "Nói để search", "음성 검색", R.drawable.ms_travel_explore),
            ToolPresetItem("quick_record", "Quick Record", "Thu âm nhanh", "빠른 녹음", R.drawable.ms_screen_record),
        ),
    ),
    // Column 3b: Device Audio presets
    ToolCategory(
        labelGetter = { it.toolsCategoryDeviceAudio },
        accentColorToken = { it.statusWarning },
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO),
        presets = listOf(
            ToolPresetItem("study_language", "Study language", "Học ngoại ngữ", "언어 학습", R.drawable.ms_school),
            ToolPresetItem("record_device", "Device Record", "Thu âm máy", "시스템 녹음", R.drawable.ms_surround_sound),
            ToolPresetItem("transcribe_english_offline", "Transcribe English", "Chép lời TA", "영어 받아쓰기", R.drawable.ms_speech_to_text),
        ),
    ),
)

@Composable
internal fun ToolsSection(
    locale: MobileLocaleText,
    onPresetClick: (String) -> Unit = {},
    onPagerSwipeLockChanged: (Boolean) -> Unit = {},
    modifier: Modifier = Modifier,
) {
    val presetRepository = (LocalContext.current.applicationContext as SgtMobileApplication)
        .appContainer
        .presetRepository
    val presetCatalog by presetRepository.catalogState.collectAsState()
    val favoritePresetIds by remember(presetCatalog) {
        derivedStateOf {
            presetCatalog.presets
                .filter { it.preset.isFavorite }
                .map { it.preset.id }
                .toSet()
        }
    }
    val lang = locale.languageOptions.firstOrNull { it.label.contains("English") }?.let { null }
        ?: locale.let {
            when {
                it.turnOn == "Bật" -> "vi"
                it.turnOn == "켜기" -> "ko"
                else -> "en"
            }
        }
    val sgtColors = MaterialTheme.sgtColors
    var toolbarMode by remember { mutableStateOf(ToolbarMode.NONE) }
    var fabMenuExpanded by rememberSaveable { mutableStateOf(false) }
    var showHelpDialog by remember { mutableStateOf(false) }

    if (showHelpDialog) {
        val helpTitle = when (lang) {
            "vi" -> "Hướng dẫn sử dụng"
            "ko" -> "사용 가이드"
            else -> "Quick Guide"
        }
        val helpBubbleTitle = when (lang) {
            "vi" -> "Bong bóng Quick Settings"
            "ko" -> "Quick Settings 버블"
            else -> "Quick Settings Bubble"
        }
        val helpBubbleDesc = when (lang) {
            "vi" -> "Thêm mục bong bóng SGT vào Quick Settings, bật bong bóng và cấp quyền overlay (có thể phải mở khoá Restricted settings của SGT trước), một bong bóng nổi sẽ xuất hiện trên màn hình. Nhấn vào để mở bảng công cụ yêu thích và dùng tại bất kỳ ứng dụng nào."
            "ko" -> "Quick Settings에 SGT 버블 타일을 추가하고, 버블을 켜고 오버레이 권한을 부여하세요 (먼저 SGT의 제한된 설정을 해제해야 할 수 있습니다). 화면에 플로팅 버블이 나타납니다. 탭하면 즐겨찾기 도구 패널이 열리고 어떤 앱에서든 바로 사용할 수 있습니다."
            else -> "Add the SGT bubble tile to Quick Settings, enable the bubble and grant overlay permission (you may need to unlock Restricted settings for SGT first). A floating bubble will appear on your screen. Tap it to open your favorite tools panel and use them from any app."
        }
        val helpFavTitle = when (lang) {
            "vi" -> "Đánh dấu yêu thích"
            "ko" -> "즐겨찾기 추가"
            else -> "Favoriting Tools"
        }
        val helpFavDesc = when (lang) {
            "vi" -> "Nhấn nút ★ ở thanh công cụ bên dưới, sau đó đánh dấu vào công cụ ưa thích để thêm/xóa yêu thích. Các tool yêu thích sẽ hiển thị trong bong bóng nổi. Một số công cụ sẽ yêu cầu bật Dịch vụ trợ năng lần đầu cho SGT."
            "ko" -> "하단 툴바의 ★ 버튼을 누른 후 각 도구 카드의 배지를 탭하여 즐겨찾기를 추가/제거하세요. 즐겨찾기 도구는 플로팅 버블에 표시됩니다. 일부 도구는 처음 사용 시 SGT 접근성 서비스를 켜야 합니다."
            else -> "Tap the ★ button in the bottom toolbar, then tap the badge on each tool card to add/remove favorites. Favorited tools appear in the floating bubble. Some tools will require enabling the Accessibility Service for SGT on first use."
        }
        val helpDismiss = when (lang) {
            "vi" -> "Đã hiểu"
            "ko" -> "알겠습니다"
            else -> "Got it"
        }
        androidx.compose.material3.AlertDialog(
            onDismissRequest = { showHelpDialog = false },
            icon = { Icon(painterResource(R.drawable.ms_help_outline), contentDescription = null, tint = MaterialTheme.colorScheme.primary) },
            title = { Text(helpTitle, style = MaterialTheme.typography.headlineSmall) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(helpBubbleTitle, style = MaterialTheme.typography.titleSmall, fontWeight = androidx.compose.ui.text.font.FontWeight.Bold)
                        Text(helpBubbleDesc, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(helpFavTitle, style = MaterialTheme.typography.titleSmall, fontWeight = androidx.compose.ui.text.font.FontWeight.Bold)
                        Text(helpFavDesc, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            },
            confirmButton = {
                androidx.compose.material3.TextButton(onClick = { showHelpDialog = false }) {
                    Text(helpDismiss)
                }
            },
        )
    }

    if (fabMenuExpanded) {
        androidx.activity.compose.BackHandler { fabMenuExpanded = false }
    }

    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE

    Box(modifier = modifier.fillMaxSize()) {
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 0.dp),
            contentPadding = PaddingValues(
                bottom = 136.dp,
                end = if (isLandscape) 24.dp else 0.dp,
            ),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            items(toolCategories) { category ->
                // Merge static presets with custom presets from catalog
                val customItems = presetCatalog.presets
                    .filter { !it.isBuiltIn && it.preset.presetType in category.acceptsTypes }
                    .map { resolved ->
                        ToolPresetItem(
                            id = resolved.preset.id,
                            nameEn = resolved.preset.nameEn,
                            nameVi = resolved.preset.nameVi,
                            nameKo = resolved.preset.nameKo,
                            icon = R.drawable.ms_auto_awesome,
                            isFullId = true,
                        )
                    }
                // Filter out hidden/deleted built-in presets
                val catalogIds = presetCatalog.presets.map { it.preset.id }.toSet()
                val visibleBuiltIns = category.presets.filter { "preset_${it.id}" in catalogIds }
                val effectivePresets = visibleBuiltIns + customItems
                ToolCategoryRow(
                    label = category.labelGetter(locale),
                    accentColor = category.accentColorToken(sgtColors),
                    presets = effectivePresets,
                    lang = lang,
                    onPresetClick = onPresetClick,
                    onPagerSwipeLockChanged = onPagerSwipeLockChanged,
                    toolbarMode = toolbarMode,
                    favoritePresetIds = favoritePresetIds,
                    onFavoriteToggle = { presetId ->
                        presetRepository.toggleFavorite(presetId)
                    },
                    onDuplicate = { presetId ->
                        val newId = presetRepository.duplicatePreset(presetId, lang)
                        android.util.Log.d("PresetTools", "DUPLICATE preset=$presetId → newId=$newId")
                        android.util.Log.d("PresetTools", "Catalog size after: ${presetRepository.catalogState.value.presets.size}")
                        android.util.Log.d("PresetTools", "Custom presets: ${presetRepository.catalogState.value.presets.filter { !it.isBuiltIn }.map { it.preset.nameEn }}")
                    },
                    onDelete = { presetId ->
                        android.util.Log.d("PresetTools", "DELETE preset=$presetId")
                        presetRepository.deletePreset(presetId)
                        android.util.Log.d("PresetTools", "Catalog size after: ${presetRepository.catalogState.value.presets.size}")
                    },
                )
            }
        }

        // Toolbar with FAB — single HorizontalFloatingToolbar, FAB in its slot
        HorizontalFloatingToolbar(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .navigationBarsPadding()
                .padding(horizontal = 8.dp, vertical = 8.dp),
            expanded = true,
            floatingActionButton = {
                MorphingCreateFab(
                    expanded = fabMenuExpanded,
                    onClick = { fabMenuExpanded = !fabMenuExpanded },
                )
            },
            content = {
                // Help button on the left
                ToolbarModeButton(
                    active = false,
                    icon = R.drawable.ms_help_outline,
                    contentDescription = when (lang) { "vi" -> "Trợ giúp"; "ko" -> "도움말"; else -> "Help" },
                    activeContainer = MaterialTheme.colorScheme.primary,
                    activeContent = MaterialTheme.colorScheme.onPrimary,
                    onClick = { showHelpDialog = true },
                )

                // Each button: icon only when inactive, icon + label when active
                data class ToolAction(
                    val mode: ToolbarMode,
                    @DrawableRes val icon: Int,
                    val label: String,
                    val activeContainer: Color,
                    val activeContent: Color,
                )
                val actions = listOf(
                    ToolAction(ToolbarMode.DUPLICATE, R.drawable.ms_content_copy,
                        when (lang) { "vi" -> "Nhân bản"; "ko" -> "복제"; else -> "Duplicate" },
                        MaterialTheme.colorScheme.primary,
                        MaterialTheme.colorScheme.onPrimary),
                    ToolAction(ToolbarMode.FAVORITE, R.drawable.ms_star,
                        when (lang) { "vi" -> "Yêu thích"; "ko" -> "즐겨찾기"; else -> "Favorite" },
                        MaterialTheme.colorScheme.primary,
                        MaterialTheme.colorScheme.onPrimary),
                    ToolAction(ToolbarMode.DELETE, R.drawable.ms_delete,
                        when (lang) { "vi" -> "Xóa"; "ko" -> "삭제"; else -> "Delete" },
                        MaterialTheme.colorScheme.error,
                        MaterialTheme.colorScheme.onError),
                )
                actions.forEach { action ->
                    ToolbarModeButton(
                        active = toolbarMode == action.mode,
                        icon = action.icon,
                        contentDescription = action.label,
                        activeContainer = action.activeContainer,
                        activeContent = action.activeContent,
                        onClick = {
                            val isActive = toolbarMode == action.mode
                            toolbarMode = if (isActive) ToolbarMode.NONE else action.mode
                        },
                    )
                }
            },
        )

        if (fabMenuExpanded) androidx.activity.compose.BackHandler { fabMenuExpanded = false }
        data class CreateOption(val type: dev.screengoated.toolbox.mobile.shared.preset.PresetType, @DrawableRes val icon: Int, val label: String, val accentColor: Color)
        val createOptions = listOf(
            CreateOption(dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE, R.drawable.ms_image, locale.toolsCategoryImage, sgtColors.statusProcessing),
            CreateOption(dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT, R.drawable.ms_text_fields, locale.toolsCategoryTextSelect, sgtColors.statusSuccess),
            CreateOption(dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT, R.drawable.ms_keyboard, locale.toolsCategoryTextInput, sgtColors.statusSuccess),
            CreateOption(dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC, R.drawable.ms_mic, locale.toolsCategoryMicRecording, sgtColors.statusWarning),
            CreateOption(dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO, R.drawable.ms_speaker_phone, locale.toolsCategoryDeviceAudio, sgtColors.statusWarning),
        )

        if (isLandscape) {
            // Landscape: DropdownMenu (handles overflow with scroll, no clipping)
            Box(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .navigationBarsPadding()
                    .padding(end = 8.dp, bottom = 56.dp),
            ) {
                androidx.compose.material3.DropdownMenu(
                    expanded = fabMenuExpanded,
                    onDismissRequest = { fabMenuExpanded = false },
                ) {
                    createOptions.forEach { opt ->
                        val optionContentColor = opt.accentColor
                        androidx.compose.material3.DropdownMenuItem(
                            onClick = {
                                fabMenuExpanded = false
                                presetRepository.createCustomPreset(type = opt.type, lang = lang)
                            },
                            leadingIcon = { Icon(painterResource(opt.icon), null, modifier = Modifier.size(18.dp), tint = opt.accentColor) },
                            text = { Text(opt.label, style = MaterialTheme.typography.labelLarge, color = optionContentColor) },
                        )
                    }
                }
            }
        } else {
            // Portrait: M3E FloatingActionButtonMenu (full animation)
            FloatingActionButtonMenu(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .navigationBarsPadding()
                    .padding(end = 8.dp, bottom = 72.dp),
                expanded = fabMenuExpanded,
                button = {},
            ) {
                createOptions.forEachIndexed { i, opt ->
                    val optionContentColor = menuContentColor(opt.accentColor)
                    FloatingActionButtonMenuItem(
                        onClick = {
                            fabMenuExpanded = false
                            presetRepository.createCustomPreset(type = opt.type, lang = lang)
                        },
                        icon = { Icon(painterResource(opt.icon), null, tint = optionContentColor) },
                        text = { Text(opt.label, style = MaterialTheme.typography.labelLarge, color = optionContentColor) },
                        modifier = if (i == createOptions.lastIndex) Modifier.padding(bottom = 12.dp) else Modifier,
                        containerColor = opt.accentColor,
                        contentColor = optionContentColor,
                    )
                }
            }
        }
    }
}

internal enum class ToolbarMode { NONE, DUPLICATE, FAVORITE, DELETE }

@Composable
private fun MorphingCreateFab(
    expanded: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val morphProgress by animateFloatAsState(if (expanded) 1f else 0f, spring(dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy, stiffness = androidx.compose.animation.core.Spring.StiffnessLow), label = "tools-create-morph")
    val rotation by animateFloatAsState(if (expanded) 36f else 0f, spring(dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy, stiffness = androidx.compose.animation.core.Spring.StiffnessLow), label = "tools-create-rotation")
    val morph = remember { Morph(MaterialShapes.Square, MaterialShapes.Cookie4Sided) }
    val containerColor = MaterialTheme.colorScheme.primary
    val iconColor = MaterialTheme.colorScheme.onPrimary
    val interactionSource = remember { MutableInteractionSource() }
    val shapeRotation by animateFloatAsState(if (expanded) 22f else 0f, spring(dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy, stiffness = androidx.compose.animation.core.Spring.StiffnessLow), label = "tools-create-shape-rotation")
    val iconRotation by animateFloatAsState(if (expanded) 45f else 0f, spring(dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy, stiffness = androidx.compose.animation.core.Spring.StiffnessMediumLow), label = "tools-create-icon-rotation")

    Box(
        modifier = modifier
            .size(58.dp)
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                onClick = onClick,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(
            modifier = Modifier
                .fillMaxSize()
                .graphicsLayer { rotationZ = shapeRotation + rotation },
        ) {
            drawPath(
                path = buildMorphPath(
                    morph = morph,
                    progress = morphProgress,
                    size = size,
                    insetFraction = 0.88f,
                ),
                color = containerColor,
            )
        }
        Icon(
            painter = painterResource(R.drawable.ms_add),
            contentDescription = "Create",
            tint = iconColor,
            modifier = Modifier
                .size(24.dp)
                .graphicsLayer { rotationZ = iconRotation },
        )
    }
}

@Composable
private fun ToolbarModeButton(
    active: Boolean,
    @DrawableRes icon: Int,
    contentDescription: String,
    activeContainer: Color,
    activeContent: Color,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val progress by animateFloatAsState(if (active) 1f else 0f, spring(dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy, stiffness = androidx.compose.animation.core.Spring.StiffnessMediumLow), label = "tools-mode-progress-$contentDescription")
    val containerColor by animateColorAsState(if (active) activeContainer else MaterialTheme.colorScheme.surfaceContainerHighest.copy(alpha = 0.56f), label = "tools-mode-bg-$contentDescription")
    val iconTint by animateColorAsState(if (active) activeContent else MaterialTheme.colorScheme.onSurfaceVariant, label = "tools-mode-icon-$contentDescription")
    val buttonSize by animateFloatAsState(if (active) 52f else 40f, spring(dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy, stiffness = androidx.compose.animation.core.Spring.StiffnessMediumLow), label = "tools-mode-size-$contentDescription")
    val morph = remember { Morph(MaterialShapes.Square, MaterialShapes.Cookie6Sided) }
    val interactionSource = remember { MutableInteractionSource() }

    Box(
        modifier = modifier.size(buttonSize.dp),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(
            modifier = Modifier
                .fillMaxSize()
                .clickable(
                    interactionSource = interactionSource,
                    indication = null,
                    onClick = onClick,
                ),
        ) {
            drawPath(
                path = buildMorphPath(
                    morph = morph,
                    progress = progress,
                    size = size,
                    insetFraction = 0.9f,
                ),
                color = containerColor,
            )
        }
        Icon(
            painter = painterResource(icon),
            contentDescription = contentDescription,
            modifier = Modifier.size(if (active) 22.dp else 20.dp),
            tint = iconTint,
        )
    }
}

private fun buildMorphPath(
    morph: Morph,
    progress: Float,
    size: Size,
    insetFraction: Float,
): androidx.compose.ui.graphics.Path {
    val androidPath = morph.toPath(progress, android.graphics.Path())
    val bounds = android.graphics.RectF()
    androidPath.computeBounds(bounds, true)
    val pathWidth = max(bounds.width(), 1f)
    val pathHeight = max(bounds.height(), 1f)
    val scale = min(size.width / pathWidth, size.height / pathHeight) * insetFraction
    val matrix = android.graphics.Matrix().apply {
        postTranslate(-bounds.centerX(), -bounds.centerY())
        postScale(scale, scale)
        postTranslate(size.width / 2f, size.height / 2f)
    }
    androidPath.transform(matrix)
    return androidPath.asComposePath()
}

private fun menuContentColor(accentColor: Color): Color = if (accentColor.luminance() > 0.55f) Color(0xFF11131A) else Color.White

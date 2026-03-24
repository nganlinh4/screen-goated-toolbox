@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.background
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Note
import androidx.compose.material.icons.automirrored.rounded.TextSnippet
import androidx.compose.material.icons.automirrored.rounded.VolumeUp
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.AutoFixHigh
import androidx.compose.material.icons.rounded.CameraAlt
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.ContentCut
import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material.icons.rounded.Description
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.material.icons.rounded.FiberSmartRecord
import androidx.compose.material.icons.rounded.FormatQuote
import androidx.compose.material.icons.rounded.GTranslate
import androidx.compose.material.icons.rounded.Gamepad
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.Image
import androidx.compose.material.icons.rounded.ImageSearch
import androidx.compose.material.icons.rounded.Keyboard
import androidx.compose.material.icons.rounded.Lightbulb
import androidx.compose.material.icons.rounded.Mic
import androidx.compose.material.icons.rounded.PhotoCamera
import androidx.compose.material.icons.rounded.QrCodeScanner
import androidx.compose.material.icons.rounded.QuestionAnswer
import androidx.compose.material.icons.rounded.RecordVoiceOver
import androidx.compose.material.icons.rounded.School
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.SmartToy
import androidx.compose.material.icons.rounded.SpeakerPhone
import androidx.compose.material.icons.rounded.Spellcheck
import androidx.compose.material.icons.rounded.Star
import androidx.compose.material.icons.rounded.Summarize
import androidx.compose.material.icons.rounded.SwapHoriz
import androidx.compose.material.icons.rounded.TableChart
import androidx.compose.material.icons.rounded.TextFields
import androidx.compose.material.icons.rounded.Translate
import androidx.compose.material.icons.rounded.Verified
import androidx.compose.material.icons.rounded.VoiceChat
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FloatingActionButtonMenu
import androidx.compose.material3.FloatingActionButtonMenuItem
import androidx.compose.material3.FloatingToolbarDefaults
import androidx.compose.material3.HorizontalFloatingToolbar
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.theme.SgtExtendedColors
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors

internal data class ToolPresetItem(
    val id: String,
    val nameEn: String,
    val nameVi: String,
    val nameKo: String,
    val icon: ImageVector,
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
            ToolPresetItem("translate", "Translate region", "Dịch vùng", "영역 번역", Icons.Rounded.Translate),
            ToolPresetItem("extract_retranslate", "Trans (ACCURATE)", "Dịch vùng (CHUẨN)", "영역 번역 (정확)", Icons.Rounded.Verified),
            ToolPresetItem("translate_auto_paste", "Trans (Auto paste)", "Dịch vùng (Tự dán)", "영역 번역 (자동 붙.)", Icons.Rounded.ContentCut),
            ToolPresetItem("extract_table", "Extract Table", "Trích bảng", "표 추출", Icons.Rounded.TableChart),
            ToolPresetItem("translate_retranslate", "Trans+Retrans", "Dịch vùng+Dịch lại", "번역+재번역", Icons.Rounded.Translate),
            ToolPresetItem("extract_retrans_retrans", "Trans (ACC)+Retrans", "D.vùng (CHUẨN)+D.lại", "번역(정확)+재번역", Icons.Rounded.Verified),
            ToolPresetItem("ocr", "Extract text", "Lấy text từ ảnh", "텍스트 추출", Icons.Rounded.TextFields),
            ToolPresetItem("ocr_read", "Read this region", "Đọc vùng này", "영역 읽기", Icons.AutoMirrored.Rounded.VolumeUp),
            ToolPresetItem("quick_screenshot", "Quick Screenshot", "Chụp MH nhanh", "빠른 스크린샷", Icons.Rounded.PhotoCamera),
            ToolPresetItem("qr_scanner", "QR Scanner", "Quét mã QR", "QR 스캔", Icons.Rounded.QrCodeScanner),
            ToolPresetItem("summarize", "Summarize region", "Tóm tắt vùng", "영역 요약", Icons.Rounded.Summarize),
            ToolPresetItem("desc", "Describe image", "Mô tả ảnh", "이미지 설명", Icons.Rounded.Description),
            ToolPresetItem("ask_image", "Ask about image", "Hỏi về ảnh", "이미지 질문", Icons.Rounded.ImageSearch),
            ToolPresetItem("fact_check", "Fact Check", "Kiểm chứng thông tin", "정보 확인", Icons.Rounded.Verified),
            ToolPresetItem("omniscient_god", "Omniscient God", "Thần Trí tuệ", "전지전능", Icons.Rounded.AutoAwesome),
            ToolPresetItem("hang_image", "Image Overlay", "Treo ảnh", "이미지 오버레이", Icons.Rounded.CameraAlt),
        ),
    ),
    // Column 2a: Text Select presets
    ToolCategory(
        labelGetter = { it.toolsCategoryTextSelect },
        accentColorToken = { it.statusSuccess },
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT),
        presets = listOf(
            ToolPresetItem("read_aloud", "Read aloud", "Đọc to", "크게 읽기", Icons.Rounded.RecordVoiceOver),
            ToolPresetItem("translate_select", "Translate", "Dịch", "번역", Icons.Rounded.GTranslate),
            ToolPresetItem("translate_arena", "Trans (Arena)", "Dịch (Arena)", "번역 (아레나)", Icons.Rounded.Translate),
            ToolPresetItem("trans_retrans_select", "Trans+Retrans", "Dịch+Dịch lại", "번역+재번역", Icons.Rounded.Translate),
            ToolPresetItem("select_translate_replace", "Trans & Replace", "Dịch và Thay", "번역 후 교체", Icons.Rounded.SwapHoriz),
            ToolPresetItem("fix_grammar", "Fix Grammar", "Sửa ngữ pháp", "문법 수정", Icons.Rounded.Spellcheck),
            ToolPresetItem("rephrase", "Rephrase", "Viết lại", "다시 쓰기", Icons.Rounded.FormatQuote),
            ToolPresetItem("make_formal", "Make Formal", "Chuyên nghiệp hóa", "공식적으로", Icons.Rounded.AutoFixHigh),
            ToolPresetItem("explain", "Explain", "Giải thích", "설명", Icons.Rounded.Lightbulb),
            ToolPresetItem("ask_text", "Ask about text...", "Hỏi về text...", "텍스트 질문", Icons.Rounded.QuestionAnswer),
            ToolPresetItem("edit_as_follows", "Edit as follows:", "Sửa như sau:", "다음과 같이 수정:", Icons.Rounded.Edit),
            ToolPresetItem("101_on_this", "101 on this", "Tất tần tật", "이것의 모든 것", Icons.Rounded.School),
            ToolPresetItem("hang_text", "Text Overlay", "Treo text", "텍스트 오버레이", Icons.AutoMirrored.Rounded.TextSnippet),
        ),
    ),
    // Column 2b: Text Input (Type) presets
    ToolCategory(
        labelGetter = { it.toolsCategoryTextInput },
        accentColorToken = { it.statusSuccess },
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT),
        presets = listOf(
            ToolPresetItem("trans_retrans_typing", "Trans+Retrans (Type)", "Dịch+Dịch lại (Tự gõ)", "번역+재번역 (입력)", Icons.Rounded.Translate),
            ToolPresetItem("ask_ai", "Ask AI", "Hỏi AI", "AI 질문", Icons.Rounded.SmartToy),
            ToolPresetItem("internet_search", "Internet Search", "Tìm kiếm internet", "인터넷 검색", Icons.Rounded.Search),
            ToolPresetItem("make_game", "Make a Game", "Tạo con game", "게임 만들기", Icons.Rounded.Gamepad),
            ToolPresetItem("quick_note", "Quick Note", "Note nhanh", "빠른 메모", Icons.AutoMirrored.Rounded.Note),
        ),
    ),
    // Column 3a: Mic presets
    ToolCategory(
        labelGetter = { it.toolsCategoryMicRecording },
        accentColorToken = { it.statusWarning },
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC),
        presets = listOf(
            ToolPresetItem("transcribe", "Transcribe speech", "Lời nói thành văn", "음성 받아쓰기", Icons.Rounded.Mic),
            ToolPresetItem("continuous_writing_online", "Continuous Writing", "Viết liên tục", "연속 입력", Icons.Rounded.Keyboard),
            ToolPresetItem("fix_pronunciation", "Fix pronunciation", "Chỉnh phát âm", "발음 교정", Icons.Rounded.RecordVoiceOver),
            ToolPresetItem("transcribe_retranslate", "Quick 4NR reply 1", "Trả lời ng.nc.ngoài 1", "빠른 외국인 답변 1", Icons.Rounded.Translate),
            ToolPresetItem("quicker_foreigner_reply", "Quick 4NR reply 2", "Trả lời ng.nc.ngoài 2", "빠른 외국인 답변 2", Icons.Rounded.Translate),
            ToolPresetItem("quick_ai_question", "Quick AI Question", "Hỏi nhanh AI", "빠른 AI 질문", Icons.Rounded.VoiceChat),
            ToolPresetItem("voice_search", "Voice Search", "Nói để search", "음성 검색", Icons.Rounded.Search),
            ToolPresetItem("quick_record", "Quick Record", "Thu âm nhanh", "빠른 녹음", Icons.Rounded.FiberSmartRecord),
        ),
    ),
    // Column 3b: Device Audio presets
    ToolCategory(
        labelGetter = { it.toolsCategoryDeviceAudio },
        accentColorToken = { it.statusWarning },
        acceptsTypes = setOf(dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO),
        presets = listOf(
            ToolPresetItem("study_language", "Study language", "Học ngoại ngữ", "언어 학습", Icons.Rounded.School),
            ToolPresetItem("record_device", "Device Record", "Thu âm máy", "시스템 녹음", Icons.Rounded.SpeakerPhone),
            ToolPresetItem("transcribe_english_offline", "Transcribe English", "Chép lời TA", "영어 받아쓰기", Icons.Rounded.GraphicEq),
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
                            icon = Icons.Rounded.AutoAwesome,
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
                FloatingToolbarDefaults.VibrantFloatingActionButton(
                    onClick = { fabMenuExpanded = !fabMenuExpanded },
                    shape = MaterialTheme.shapes.large,
                ) {
                    Icon(
                        if (fabMenuExpanded) Icons.Rounded.Close else Icons.Rounded.Add,
                        contentDescription = "Create",
                    )
                }
            },
            content = {
                // Each button: icon only when inactive, icon + label when active
                data class ToolAction(
                    val mode: ToolbarMode,
                    val icon: ImageVector,
                    val label: String,
                    val activeTint: Color,
                )
                val actions = listOf(
                    ToolAction(ToolbarMode.DUPLICATE, Icons.Rounded.ContentCopy,
                        when (lang) { "vi" -> "Nhân bản"; "ko" -> "복제"; else -> "Duplicate" },
                        MaterialTheme.colorScheme.primary),
                    ToolAction(ToolbarMode.FAVORITE, Icons.Rounded.Star,
                        when (lang) { "vi" -> "Yêu thích"; "ko" -> "즐겨찾기"; else -> "Favorite" },
                        MaterialTheme.colorScheme.primary),
                    ToolAction(ToolbarMode.DELETE, Icons.Rounded.Delete,
                        when (lang) { "vi" -> "Xóa"; "ko" -> "삭제"; else -> "Delete" },
                        MaterialTheme.colorScheme.error),
                )
                actions.forEach { action ->
                    val isActive = toolbarMode == action.mode
                    val tint by animateColorAsState(
                        if (isActive) action.activeTint else MaterialTheme.colorScheme.onSurfaceVariant,
                        label = "tint-${action.mode}",
                    )
                    val bgAlpha by animateFloatAsState(
                        if (isActive) 0.12f else 0f,
                        label = "bg-${action.mode}",
                    )
                    IconButton(
                        onClick = {
                            toolbarMode = if (isActive) ToolbarMode.NONE else action.mode
                        },
                    ) {
                        Box(
                            modifier = Modifier
                                .background(
                                    action.activeTint.copy(alpha = bgAlpha),
                                    MaterialTheme.shapes.medium,
                                )
                                .padding(8.dp),
                        ) {
                            Icon(
                                action.icon,
                                contentDescription = action.label,
                                modifier = Modifier.size(20.dp),
                                tint = tint,
                            )
                        }
                    }
                }
            },
        )

        // FAB Menu — create preset options
        if (fabMenuExpanded) {
            androidx.activity.compose.BackHandler { fabMenuExpanded = false }
        }

        data class CreateOption(
            val type: dev.screengoated.toolbox.mobile.shared.preset.PresetType,
            val icon: ImageVector,
            val label: String,
        )
        val createOptions = listOf(
            CreateOption(dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE, Icons.Rounded.Image, locale.toolsCategoryImage),
            CreateOption(dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT, Icons.Rounded.TextFields, locale.toolsCategoryTextSelect),
            CreateOption(dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_INPUT, Icons.Rounded.Keyboard, locale.toolsCategoryTextInput),
            CreateOption(dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC, Icons.Rounded.Mic, locale.toolsCategoryMicRecording),
            CreateOption(dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO, Icons.Rounded.SpeakerPhone, locale.toolsCategoryDeviceAudio),
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
                        androidx.compose.material3.DropdownMenuItem(
                            onClick = {
                                fabMenuExpanded = false
                                presetRepository.createCustomPreset(type = opt.type, lang = lang)
                            },
                            leadingIcon = { Icon(opt.icon, null, modifier = Modifier.size(18.dp)) },
                            text = { Text(opt.label, style = MaterialTheme.typography.labelLarge) },
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
                    FloatingActionButtonMenuItem(
                        onClick = {
                            fabMenuExpanded = false
                            presetRepository.createCustomPreset(type = opt.type, lang = lang)
                        },
                        icon = { Icon(opt.icon, null) },
                        text = { Text(opt.label, style = MaterialTheme.typography.labelLarge) },
                        modifier = if (i == createOptions.lastIndex) Modifier.padding(bottom = 12.dp) else Modifier,
                    )
                }
            }
        }
    }
}

internal enum class ToolbarMode { NONE, DUPLICATE, FAVORITE, DELETE }

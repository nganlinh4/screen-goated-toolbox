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

// Google Sans Flex at wdth=80 — prevents long labels from wrapping in toggle buttons
internal val condensedButtonFont: androidx.compose.ui.text.font.FontFamily by lazy {
    androidx.compose.ui.text.font.FontFamily(
        androidx.compose.ui.text.font.Font(
            resId = dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
            variationSettings = androidx.compose.ui.text.font.FontVariation.Settings(
                androidx.compose.ui.text.font.FontVariation.Setting("wdth", 80f),
            ),
        ),
    )
}

// ---------------------------------------------------------------------------
// Editor type grouping: the 5 PresetType variants collapse into 3 editor tabs
// ---------------------------------------------------------------------------

internal enum class EditorTypeGroup { IMAGE, TEXT, AUDIO }

internal fun PresetType.editorGroup(): EditorTypeGroup = when (this) {
    PresetType.IMAGE -> EditorTypeGroup.IMAGE
    PresetType.TEXT_SELECT, PresetType.TEXT_INPUT -> EditorTypeGroup.TEXT
    PresetType.MIC, PresetType.DEVICE_AUDIO -> EditorTypeGroup.AUDIO
}

internal fun EditorTypeGroup.defaultPresetType(): PresetType = when (this) {
    EditorTypeGroup.IMAGE -> PresetType.IMAGE
    EditorTypeGroup.TEXT -> PresetType.TEXT_SELECT
    EditorTypeGroup.AUDIO -> PresetType.MIC
}

// ---------------------------------------------------------------------------
// Localization helpers
// ---------------------------------------------------------------------------

internal fun editorTypeLabel(group: EditorTypeGroup, lang: String): String = when (group) {
    EditorTypeGroup.IMAGE -> when (lang) {
        "vi" -> "Ảnh"
        "ko" -> "이미지"
        else -> "Image"
    }
    EditorTypeGroup.TEXT -> when (lang) {
        "vi" -> "Văn bản"
        "ko" -> "텍스트"
        else -> "Text"
    }
    EditorTypeGroup.AUDIO -> when (lang) {
        "vi" -> "Âm thanh"
        "ko" -> "오디오"
        else -> "Audio"
    }
}

@DrawableRes
internal fun editorTypeIcon(group: EditorTypeGroup): Int = when (group) {
    EditorTypeGroup.IMAGE -> R.drawable.ms_image
    EditorTypeGroup.TEXT -> R.drawable.ms_text_fields
    EditorTypeGroup.AUDIO -> R.drawable.ms_audio_file
}

internal fun localized(lang: String, en: String, vi: String, ko: String): String = when (lang) {
    "vi" -> vi
    "ko" -> ko
    else -> en
}

// ---------------------------------------------------------------------------
// Main screen
// ---------------------------------------------------------------------------

@Composable
fun PresetEditorScreen(
    preset: Preset,
    lang: String,
    onBack: () -> Unit,
    onPresetChanged: (Preset) -> Unit = {},
    onRestoreDefault: () -> Unit = {},
    providerSettings: dev.screengoated.toolbox.mobile.preset.PresetProviderSettings =
        dev.screengoated.toolbox.mobile.preset.PresetProviderSettings(),
) {
    val isBuiltIn = preset.id.startsWith("preset_")
    var editState by remember(preset) { mutableStateOf(preset.copy()) }
    var resetCounter by remember { mutableIntStateOf(0) }

    fun autoSave(newState: Preset) {
        editState = newState
        onPresetChanged(newState)
    }

    var isRenamingPreset by remember { mutableStateOf(false) }
    var renameText by remember(editState.nameEn) { mutableStateOf(editState.nameEn) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    if (isRenamingPreset && !isBuiltIn) {
                        OutlinedTextField(
                            value = renameText,
                            onValueChange = { renameText = it },
                            singleLine = true,
                            modifier = Modifier.fillMaxWidth(),
                            textStyle = MaterialTheme.typography.titleMedium,
                            trailingIcon = {
                                IconButton(onClick = {
                                    autoSave(editState.copy(nameEn = renameText))
                                    isRenamingPreset = false
                                }) { Icon(painterResource(R.drawable.ms_check), contentDescription = null) }
                            },
                        )
                    } else {
                        Text(
                            text = editState.name(lang),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = if (!isBuiltIn) Modifier.clickable { isRenamingPreset = true } else Modifier,
                        )
                    }
                },
                navigationIcon = {
                    IconButton(onClick = { if (isRenamingPreset) isRenamingPreset = false else onBack() }) {
                        Icon(painterResource(R.drawable.ms_arrow_back), contentDescription = null)
                    }
                },
                actions = {
                    if (isBuiltIn) {
                        IconButton(onClick = {
                            onRestoreDefault()
                            resetCounter++
                        }) {
                            Icon(painterResource(R.drawable.ms_settings_backup_restore), contentDescription = localized(lang, "Restore", "Khôi phục", "복원"), tint = MaterialTheme.colorScheme.primary)
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.surface),
            )
        },
    ) { padding ->
        val windowInfo = androidx.compose.ui.platform.LocalWindowInfo.current
        val density = androidx.compose.ui.platform.LocalDensity.current
        val windowWidth = with(density) { windowInfo.containerSize.width.toDp() }
        val windowHeight = with(density) { windowInfo.containerSize.height.toDp() }
        val isLandscape = windowWidth > windowHeight

        if (isLandscape) {
            Row(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .padding(horizontal = 16.dp)
                    .padding(bottom = 16.dp),
                horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                // Left column: Header + Mode selectors
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    HeaderSection(
                        editState = editState,
                        lang = lang,
                        isBuiltIn = isBuiltIn,
                        onNameChanged = { autoSave(editState.copy(nameEn = it)) },
                        onTypeGroupChanged = { group ->
                            autoSave(editState.copy(presetType = group.defaultPresetType()))
                        },
                        onControllerToggled = { autoSave(editState.copy(showControllerUi = it)) },
                    )
                    ModeSelectorsSection(
                        editState = editState,
                        lang = lang,
                        controllerOn = editState.showControllerUi || editState.isMaster,
                        onUpdate = { autoSave(it) },
                    )
                    if (editState.showControllerUi || editState.isMaster) {
                        MasterDescriptionSection(lang = lang)
                    }
                    if (editState.audioProcessingMode == "realtime") {
                        RealtimeDescriptionSection(lang = lang)
                    }
                }
                // Right column: Node graph + Processing chain
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    if (!editState.showControllerUi && !editState.isMaster && editState.audioProcessingMode != "realtime") {
                        NodeGraphSection(
                            editState = editState,
                            lang = lang,
                            onUpdate = { autoSave(it) },
                            resetCounter = resetCounter,
                            providerSettings = providerSettings,
                        )
                    }
                    val hasAnyCopy = editState.blocks.any {
                        it.blockType != BlockType.INPUT_ADAPTER && it.autoCopy
                    }
                    if (hasAnyCopy && !editState.showControllerUi) {
                        AutoPasteSection(
                            editState = editState,
                            lang = lang,
                            onUpdate = { autoSave(it) },
                        )
                    }
                }
            }
        } else {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp)
                    .padding(bottom = 32.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                HeaderSection(
                    editState = editState,
                    lang = lang,
                    isBuiltIn = isBuiltIn,
                    onNameChanged = { autoSave(editState.copy(nameEn = it)) },
                    onTypeGroupChanged = { group ->
                        autoSave(editState.copy(presetType = group.defaultPresetType()))
                    },
                    onControllerToggled = { autoSave(editState.copy(showControllerUi = it)) },
                )
                ModeSelectorsSection(
                    editState = editState,
                    lang = lang,
                    controllerOn = editState.showControllerUi || editState.isMaster,
                    onUpdate = { autoSave(it) },
                )
                if (editState.showControllerUi || editState.isMaster) {
                    MasterDescriptionSection(lang = lang)
                }
                if (editState.audioProcessingMode == "realtime") {
                    RealtimeDescriptionSection(lang = lang)
                }
                if (!editState.showControllerUi && !editState.isMaster && editState.audioProcessingMode != "realtime") {
                    NodeGraphSection(
                        editState = editState,
                        lang = lang,
                        onUpdate = { autoSave(it) },
                        resetCounter = resetCounter,
                        providerSettings = providerSettings,
                    )
                }
                val hasAnyCopy = editState.blocks.any {
                    it.blockType != BlockType.INPUT_ADAPTER && it.autoCopy
                }
                if (hasAnyCopy && !editState.showControllerUi) {
                    AutoPasteSection(
                        editState = editState,
                        lang = lang,
                        onUpdate = { autoSave(it) },
                    )
                }
            }
        }
    }
}

// =============================================================================
// Section 1: Header
// =============================================================================


package dev.screengoated.toolbox.mobile.ui.i18n

import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import java.util.Locale
import kotlin.random.Random

/**
 * Localized text catalog for the mobile shell.
 *
 * The per-language string tables live in sibling files ([englishMobileLocaleText],
 * [vietnameseMobileLocaleText], [koreanMobileLocaleText]) so this file stays focused on the
 * shape of the catalog and the locale-aware helpers. Shared value types are in
 * [MobileLocaleTypes].
 */
data class MobileLocaleText(
    val appTitle: String,
    val appHeaderTitle: String,
    val appSubtitle: String,
    val shellSectionTitle: String,
    val shellUtilitiesTitle: String,
    val shellCurrentSectionLabel: String,
    val shellAppsLabel: String,
    val shellAppsDescription: String,
    val shellToolsLabel: String,
    val shellToolsDescription: String,
    val shellSettingsLabel: String,
    val shellSettingsDescription: String,
    val shellHistoryLabel: String,
    val shellHistoryDescription: String,
    val historyTitle: String,
    val historyMaxItemsLabel: String,
    val historyRetentionHint: String,
    val historySearchLabel: String,
    val historySearchPlaceholder: String,
    val historyClearSearch: String,
    val historyFolderUnavailable: String,
    val historyOpenFolder: String,
    val historyClearAll: String,
    val historyEmpty: String,
    val historyCopiedText: String,
    val historyOpenFailed: String,
    val historyCopyText: String,
    val historyDelete: String,
    val historyViewImage: String,
    val historyListenAudio: String,
    val historyViewText: String,
    val shellDownloadedToolsLabel: String,
    val shellDownloadedToolsDescription: String,
    val shellHelpLabel: String,
    val shellHelpDescription: String,
    val helpAssistantTitle: String,
    val helpAssistantQuestionLabel: String,
    val helpAssistantAskButton: String,
    val helpAssistantHint: String,
    val helpAssistantQuickOption: String,
    val helpAssistantDetailedOption: String,
    val helpAssistantScreenRecordOption: String,
    val helpAssistantAndroidOption: String,
    val helpAssistantRestOption: String,
    val helpAssistantScreenRecordPlaceholder: String,
    val helpAssistantAndroidPlaceholder: String,
    val helpAssistantRestPlaceholder: String,
    val helpAssistantScreenRecordLoading: String,
    val helpAssistantAndroidLoading: String,
    val helpAssistantRestLoading: String,
    val shellPlaceholderBadge: String,
    val shellPlaceholderMessage: String,
    val shellCredentialsTitle: String,
    val shellCredentialsDescription: String,
    val shellLiveTitle: String,
    val shellLiveDescription: String,
    val appVideoDownloaderTitle: String,
    val appDjTitle: String,
    val appTranslationGummyTitle: String,
    val translationGummyTitle: String,
    val translationGummyFirstProfile: String,
    val translationGummySecondProfile: String,
    val translationGummyLanguageLabel: String,
    val translationGummyAccentLabel: String,
    val translationGummyToneLabel: String,
    val translationGummyApply: String,
    val translationGummyStart: String,
    val translationGummyStop: String,
    val translationGummyTranscriptTitle: String,
    val translationGummyInputChip: String,
    val translationGummyOutputChip: String,
    val translationGummyNoTranscriptYet: String,
    val translationGummyStatusNotConfigured: String,
    val translationGummyStatusConnecting: String,
    val translationGummyStatusReady: String,
    val translationGummyStatusReconnecting: String,
    val translationGummyStatusError: String,
    val translationGummyStatusStopped: String,
    val translationGummyApiKeyRequired: String,
    val translationGummyConnectionLost: String,
    val translationGummyGuide: String,
    val translationGummyGuideOk: String,
    val translationGummyNotificationChannel: String,
    val translationGummyNotificationDescription: String,
    val translationGummyNotificationStop: String,
    val toolsCategoryImage: String,
    val toolsCategoryTextSelect: String,
    val toolsCategoryTextInput: String,
    val toolsCategoryMicRecording: String,
    val toolsCategoryDeviceAudio: String,
    val comingSoonLabel: String,
    val shellVoiceTitle: String,
    val shellVoiceDescription: String,
    val shellStatusIdle: String,
    val shellStatusActive: String,
    val shellStatusStarting: String,
    val shellStatusTranslating: String,
    val geminiKeyLabel: String,
    val cerebrasKeyLabel: String,
    val groqKeyLabel: String,
    val openRouterKeyLabel: String,
    val ollamaUrlLabel: String,
    val geminiGetKeyLink: String,
    val cerebrasGetKeyLink: String,
    val groqGetKeyLink: String,
    val openRouterGetKeyLink: String,
    val ollamaLearnMoreLink: String,
    val turnOn: String,
    val turnOff: String,
    val voiceSettingsButton: String,
    val presetRuntimeTitle: String,
    val presetRuntimeDescription: String,
    val presetRuntimeButton: String,
    val presetRuntimeSettingsAction: String,
    val usageStatsButton: String,
    val usageStatsTitle: String,
    val usageStatsModel: String,
    val usageStatsRemaining: String,
    val usageStatsUnlimited: String,
    val usageStatsNoData: String,
    val usageStatsSettingsAction: String,
    val softwareUpdateHeader: String,
    val currentVersionLabel: String,
    val checkForUpdatesButton: String,
    val checkingGithub: String,
    val upToDateLabel: String,
    val checkAgainButton: String,
    val newVersionAvailableLabel: String,
    val releaseNotesLabel: String,
    val downloadUpdateButton: String,
    val updateFailedLabel: String,
    val retryButton: String,
    val updateAvailableNotification: String,
    val usageTipsTitle: String,
    val usageTipsClickHint: String,
    val usageTipsList: List<String>,
    val resetDefaultsButton: String,
    val resetDefaultsAction: String,
    val resetDefaultsConfirmTitle: String,
    val resetDefaultsConfirmMessage: String,
    val presetRuntimeProvidersLabel: String,
    val presetRuntimeImageChainLabel: String,
    val presetRuntimeTextChainLabel: String,
    val presetRuntimeChainHint: String,
    val presetRuntimeSave: String,
    val presetRuntimeCancel: String,
    val presetRuntimeAddModel: String,
    val presetRuntimeRestoreDefault: String,
    val presetRuntimeChosenModel: String,
    val presetRuntimeChosenHint: String,
    val presetRuntimeAuto: String,
    val presetRuntimeAutoHint: String,
    val customModelsButton: String,
    val customModelsTitle: String,
    val customModelsDescription: String,
    val customModelsAddOpenRouter: String,
    val customModelsImportOpenRouter: String,
    val customModelsDisplayName: String,
    val customModelsApiModel: String,
    val customModelsType: String,
    val customModelsTextType: String,
    val customModelsVisionType: String,
    val customModelsSearch: String,
    val customModelsEnabled: String,
    val customModelsSave: String,
    val customModelsDelete: String,
    val themeCycleLabel: String,
    val themeModeLabels: Map<MobileThemeMode, String>,
    val languageOptions: List<MobileUiLanguageOption>,
    val overlay: OverlayLocaleText,
    val overlayOpacityLabel: String,
    val ttsSettingsTitle: String,
    val ttsMethodLabel: String,
    val ttsMethodStandard: String,
    val ttsMethodFast: String,
    val ttsMethodEdge: String,
    val ttsGeminiModelLabel: String,
    val ttsGoogleTranslateTitle: String,
    val ttsGoogleTranslateDesc: String,
    val ttsEdgeTitle: String,
    val ttsEdgeDesc: String,
    val ttsPitchLabel: String,
    val ttsRateLabel: String,
    val ttsVolumeLabel: String,
    val ttsVoicePerLanguageLabel: String,
    val ttsLoadingVoices: String,
    val ttsFailedLoadVoicesTemplate: String,
    val ttsRetryLabel: String,
    val ttsInitializingVoices: String,
    val ttsAddLanguageLabel: String,
    val ttsResetToDefaultsLabel: String,
    val ttsSpeedLabel: String,
    val ttsSpeedNormal: String,
    val ttsSpeedSlow: String,
    val ttsSpeedFast: String,
    val ttsVoiceLabel: String,
    val ttsPreviewAction: String,
    val ttsPreviewTexts: List<String>,
    val ttsMale: String,
    val ttsFemale: String,
    val ttsInstructionsLabel: String,
    val ttsInstructionsHint: String,
    val ttsAddCondition: String,
    val instructionLabel: String,
    val removeLabel: String,
    val closeLabel: String,
    val noLanguageConditionsYet: String,
    // Downloader
    val dlTitle: String,
    val dlUrlLabel: String,
    val dlFormatLabel: String,
    val dlStartBtn: String,
    val dlStartNowBtn: String,
    val dlOpenFile: String,
    val dlOpenFolder: String,
    val dlStatusStarting: String,
    val dlStatusFinished: String,
    val dlStatusError: String,
    val dlDepsRequired: String,
    val dlDepsInstall: String,
    val dlDepsReady: String,
    val dlDepsNotInstalled: String,
    val dlDepsDownloading: String,
    val dlDepsExtracting: String,
    val dlDepsChecking: String,
    val dlCancel: String,
    val dlChangeFolder: String,
    val dlDeleteDeps: String,
    val dlAdvanced: String,
    val dlOptMetadata: String,
    val dlOptSponsorblock: String,
    val dlOptSubtitles: String,
    val dlOptPlaylist: String,
    val dlVideoLabel: String,
    val dlAudioLabel: String,
    val dlQualityLabel: String,
    val dlSubtitleLabel: String,
    val dlBest: String,
    val dlAuto: String,
    val dlScanning: String,
    val dlSubsFoundHeader: String,
    val dlSubsNoneFound: String,
    val dlRetry: String,
    val dlShowLog: String,
    val dlHideLog: String,
    val dlReset: String,
    // Downloaded tools UI
    val toolDescYtdlp: String,
    val toolDescFfmpeg: String,
    val toolRuntimeMoonshine: String,
    val toolRuntimeMoonshineDesc: String,
    val toolRuntimeOrt: String,
    val toolRuntimeOrtDesc: String,
    val toolRuntimeSherpa: String,
    val toolRuntimeSherpaDesc: String,
    val toolUpdate: String,
    val toolDelete: String,
    val toolUpdated: String,
    val toolUpToDate: String,
    val toolUpdating: String,
    val toolUpdateFailed: String,
) {
    fun ttsFailedLoadVoices(error: String): String {
        return ttsFailedLoadVoicesTemplate.replace("{}", error)
    }

    fun compactOverlayOpacityLabel(): String {
        return selectByCurrentLocale(
            en = "Overlay opacity",
            vi = "Độ mờ overlay",
            ko = "오버레이 불투명도",
        )
    }

    fun resetDefaultsDoneMessage(): String {
        return selectByCurrentLocale(
            en = "Defaults restored",
            vi = "Đã khôi phục mặc định",
            ko = "기본값으로 복원됨",
        )
    }

    fun toolsHelpCopy(): MobileToolsHelpCopy {
        return when (languageCode()) {
            "vi" -> MobileToolsHelpCopy(
                title = "Hướng dẫn sử dụng",
                bubbleTitle = "Bong bóng Quick Settings",
                bubbleDescription = "Thêm mục bong bóng SGT vào Quick Settings, bật bong bóng và cấp quyền overlay (có thể phải mở khoá Restricted settings của SGT trước), một bong bóng nổi sẽ xuất hiện trên màn hình. Nhấn vào để mở bảng công cụ yêu thích và dùng tại bất kỳ ứng dụng nào.",
                favoriteTitle = "Đánh dấu yêu thích",
                favoriteDescription = "Nhấn nút ★ ở thanh công cụ bên dưới, sau đó đánh dấu vào công cụ ưa thích để thêm/xóa yêu thích. Các tool yêu thích sẽ hiển thị trong bong bóng nổi. Một số công cụ sẽ yêu cầu bật Dịch vụ trợ năng lần đầu cho SGT.",
                dismiss = "Đã hiểu",
            )
            "ko" -> MobileToolsHelpCopy(
                title = "사용 가이드",
                bubbleTitle = "Quick Settings 버블",
                bubbleDescription = "Quick Settings에 SGT 버블 타일을 추가하고, 버블을 켜고 오버레이 권한을 부여하세요 (먼저 SGT의 제한된 설정을 해제해야 할 수 있습니다). 화면에 플로팅 버블이 나타납니다. 탭하면 즐겨찾기 도구 패널이 열리고 어떤 앱에서든 바로 사용할 수 있습니다.",
                favoriteTitle = "즐겨찾기 추가",
                favoriteDescription = "하단 툴바의 ★ 버튼을 누른 후 각 도구 카드의 배지를 탭하여 즐겨찾기를 추가/제거하세요. 즐겨찾기 도구는 플로팅 버블에 표시됩니다. 일부 도구는 처음 사용 시 SGT 접근성 서비스를 켜야 합니다.",
                dismiss = "알겠습니다",
            )
            else -> MobileToolsHelpCopy(
                title = "Quick Guide",
                bubbleTitle = "Quick Settings Bubble",
                bubbleDescription = "Add the SGT bubble tile to Quick Settings, enable the bubble and grant overlay permission (you may need to unlock Restricted settings for SGT first). A floating bubble will appear on your screen. Tap it to open your favorite tools panel and use them from any app.",
                favoriteTitle = "Favoriting Tools",
                favoriteDescription = "Tap the ★ button in the bottom toolbar, then tap the badge on each tool card to add/remove favorites. Favorited tools appear in the floating bubble. Some tools will require enabling the Accessibility Service for SGT on first use.",
                dismiss = "Got it",
            )
        }
    }

    fun languageCode(): String {
        return when (ttsPreviewAction) {
            "Nghe thử" -> "vi"
            "미리 듣기" -> "ko"
            else -> "en"
        }
    }

    fun nextPreviewText(
        voiceName: String,
        previousIndex: Int,
        random: Random = Random(System.currentTimeMillis()),
    ): PreviewSelection {
        if (ttsPreviewTexts.isEmpty()) {
            return PreviewSelection(index = -1, text = voiceName)
        }
        val nextIndex = if (ttsPreviewTexts.size == 1) {
            0
        } else {
            val candidate = random.nextInt(ttsPreviewTexts.size)
            if (candidate == previousIndex) {
                (candidate + 1) % ttsPreviewTexts.size
            } else {
                candidate
            }
        }
        return PreviewSelection(
            index = nextIndex,
            text = ttsPreviewTexts[nextIndex].replace("{}", voiceName),
        )
    }

    private fun selectByCurrentLocale(en: String, vi: String, ko: String): String {
        return when (languageCode()) {
            "vi" -> vi
            "ko" -> ko
            else -> en
        }
    }

    companion object {
        fun forLanguage(rawCode: String): MobileLocaleText {
            return when (rawCode.lowercase(Locale.US)) {
                "vi" -> vietnameseMobileLocaleText()
                "ko" -> koreanMobileLocaleText()
                else -> englishMobileLocaleText()
            }
        }
    }
}

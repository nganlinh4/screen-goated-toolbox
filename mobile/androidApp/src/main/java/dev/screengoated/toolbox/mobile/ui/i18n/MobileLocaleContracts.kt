package dev.screengoated.toolbox.mobile.ui.i18n

import dev.screengoated.toolbox.mobile.model.MobileThemeMode

/**
 * Small locale contracts preserve the original flat `MobileLocaleText` API through
 * Kotlin interface delegation while keeping generated JVM method signatures bounded.
 */
interface MobileShellText {
    val appTitle: String
    val appHeaderTitle: String
    val appSubtitle: String
    val shellUtilitiesTitle: String
    val shellAppsLabel: String
    val shellToolsLabel: String
    val shellSettingsLabel: String
    val shellHistoryLabel: String
    val shellDownloadedToolsLabel: String
    val shellDownloadedToolsDescription: String
    val shellHelpLabel: String
    val shellHelpDescription: String
    val shellCredentialsTitle: String
    val shellCredentialsDescription: String
    val shellLiveTitle: String
    val shellLiveDescription: String
    val appVideoDownloaderTitle: String
    val appFeatureUnsupportedTitle: String
    val appFeatureUnsupportedMessage: String
    val appDjTitle: String
    val appTranslationGummyTitle: String
    val toolsCategoryImage: String
    val toolsCategoryTextSelect: String
    val toolsCategoryTextInput: String
    val toolsCategoryMicRecording: String
    val toolsCategoryDeviceAudio: String
    val comingSoonLabel: String
    val donateHeader: String
    val donateBody: String
    val donateNote: String
    val donateVietnamese: Boolean
}

interface MobileCreationAppsText {
    val appImageTo3dTitle: String
    val appImageToSvgTitle: String
}

interface MobileHistoryText {
    val historyTitle: String
    val historyMaxItemsLabel: String
    val historyRetentionHint: String
    val historySearchLabel: String
    val historySearchPlaceholder: String
    val historyClearSearch: String
    val historyFolderUnavailable: String
    val historyOpenFolder: String
    val historyClearAll: String
    val historyEmpty: String
    val historyCopiedText: String
    val historyOpenFailed: String
    val historyCopyText: String
    val historyDelete: String
    val historyViewImage: String
    val historyListenAudio: String
    val historyViewText: String
}

interface MobileHelpText {
    val helpAssistantTitle: String
    val helpAssistantQuestionLabel: String
    val helpAssistantAskButton: String
    val helpAssistantHint: String
    val helpAssistantQuickOption: String
    val helpAssistantDetailedOption: String
    val helpAssistantScreenRecordOption: String
    val helpAssistantAndroidOption: String
    val helpAssistantRestOption: String
    val helpAssistantScreenRecordPlaceholder: String
    val helpAssistantAndroidPlaceholder: String
    val helpAssistantRestPlaceholder: String
    val helpAssistantScreenRecordLoading: String
    val helpAssistantAndroidLoading: String
    val helpAssistantRestLoading: String
    val toolsHelpCopy: MobileToolsHelpCopy
}

interface MobileTranslationGummyText {
    val translationGummyTitle: String
    val translationGummyFirstProfile: String
    val translationGummySecondProfile: String
    val translationGummyLanguageLabel: String
    val translationGummyAccentLabel: String
    val translationGummyToneLabel: String
    val translationGummyApply: String
    val translationGummyStart: String
    val translationGummyStop: String
    val translationGummyTranscriptTitle: String
    val translationGummyInputChip: String
    val translationGummyOutputChip: String
    val translationGummyNoTranscriptYet: String
    val translationGummyStatusNotConfigured: String
    val translationGummyStatusConnecting: String
    val translationGummyStatusReady: String
    val translationGummyStatusReconnecting: String
    val translationGummyStatusError: String
    val translationGummyStatusStopped: String
    val translationGummyApiKeyRequired: String
    val translationGummyConnectionLost: String
    val translationGummyGuide: String
    val translationGummyGuideOk: String
    val translationGummyNotificationChannel: String
    val translationGummyNotificationDescription: String
    val translationGummyNotificationStop: String
}

interface MobileProviderText {
    val shellVoiceTitle: String
    val shellVoiceDescription: String
    val shellStatusIdle: String
    val shellStatusActive: String
    val shellStatusStarting: String
    val shellStatusTranslating: String
    val geminiKeyLabel: String
    val cerebrasKeyLabel: String
    val groqKeyLabel: String
    val openRouterKeyLabel: String
    val ollamaUrlLabel: String
    val geminiGetKeyLink: String
    val cerebrasGetKeyLink: String
    val groqGetKeyLink: String
    val openRouterGetKeyLink: String
    val ollamaLearnMoreLink: String
    val turnOn: String
    val turnOff: String
    val voiceSettingsButton: String
}

interface MobilePresetRuntimeText {
    val presetRuntimeTitle: String
    val presetRuntimeDescription: String
    val presetRuntimeButton: String
    val presetRuntimeSettingsAction: String
    val usageStatsButton: String
    val usageStatsTitle: String
    val usageStatsModel: String
    val usageStatsRemaining: String
    val usageStatsUnlimited: String
    val usageStatsNoData: String
    val usageStatsSettingsAction: String
    val usageTipsTitle: String
    val usageTipsClickHint: String
    val usageTipsList: List<String>
    val resetDefaultsButton: String
    val resetDefaultsAction: String
    val resetDefaultsConfirmTitle: String
    val resetDefaultsConfirmMessage: String
    val presetRuntimeProvidersLabel: String
    val presetRuntimeImageChainLabel: String
    val presetRuntimeTextChainLabel: String
    val presetRuntimeChainHint: String
    val presetRuntimeSave: String
    val presetRuntimeCancel: String
    val presetRuntimeAddModel: String
    val presetRuntimeRestoreDefault: String
    val presetRuntimeChosenModel: String
    val presetRuntimeChosenHint: String
    val presetRuntimeAuto: String
    val presetRuntimeAutoHint: String
}

interface MobileUpdateText {
    val softwareUpdateHeader: String
    val currentVersionLabel: String
    val checkForUpdatesButton: String
    val checkingGithub: String
    val upToDateLabel: String
    val checkAgainButton: String
    val newVersionAvailableLabel: String
    val releaseNotesLabel: String
    val downloadUpdateButton: String
    val updateFailedLabel: String
    val retryButton: String
    val updateAvailableNotification: String
    val checkingPlayUpdates: String
    val updateDownloadingLabel: String
    val updateDownloadedLabel: String
    val restartToUpdateButton: String
    val updateNowButton: String
}

interface MobileCustomModelsText {
    val customModelsButton: String
    val customModelsTitle: String
    val customModelsDescription: String
    val customModelsBuiltinLocked: String
    val customModelsUserModels: String
    val customModelsDiscoveredModels: String
    val customModelsAddOpenRouter: String
    val customModelsAdd: String
    val customModelsImportOpenRouter: String
    val customModelsScanOllama: String
    val customModelsScan: String
    val customModelsNoModels: String
    val customModelsDisplayName: String
    val customModelsApiModel: String
    val customModelsType: String
    val customModelsTextType: String
    val customModelsVisionType: String
    val customModelsSearch: String
    val customModelsEnabled: String
    val customModelsSave: String
    val customModelsDelete: String
}

interface MobileAppearanceText {
    val themeCycleLabel: String
    val themeModeLabels: Map<MobileThemeMode, String>
    val languageOptions: List<MobileUiLanguageOption>
    val overlay: OverlayLocaleText
    val overlayOpacityLabel: String
    val compactOverlayOpacityLabel: String
    val resetDefaultsDoneMessage: String
}

interface MobileTtsSettingsText {
    val ttsSettingsTitle: String
    val ttsMethodLabel: String
    val ttsMethodStandard: String
    val ttsMethodFast: String
    val ttsMethodEdge: String
    val ttsGeminiModelLabel: String
    val ttsGoogleTranslateTitle: String
    val ttsGoogleTranslateDesc: String
    val ttsEdgeTitle: String
    val ttsEdgeDesc: String
    val ttsPitchLabel: String
    val ttsRateLabel: String
    val ttsVolumeLabel: String
    val ttsVoicePerLanguageLabel: String
    val ttsLoadingVoices: String
    val ttsFailedLoadVoicesTemplate: String
    val ttsRetryLabel: String
    val ttsInitializingVoices: String
    val ttsAddLanguageLabel: String
    val ttsResetToDefaultsLabel: String
    val instructionLabel: String
    val removeLabel: String
    val closeLabel: String
}

interface MobileTtsVoiceText {
    val ttsSpeedLabel: String
    val ttsSpeedNormal: String
    val ttsSpeedSlow: String
    val ttsSpeedFast: String
    val ttsVoiceLabel: String
    val ttsPreviewAction: String
    val ttsPreviewTexts: List<String>
    val ttsMale: String
    val ttsFemale: String
    val ttsInstructionsLabel: String
    val ttsInstructionsHint: String
    val ttsAddCondition: String
    val noLanguageConditionsYet: String
}

interface MobileDownloadText {
    val dlTitle: String
    val dlUrlLabel: String
    val dlFormatLabel: String
    val dlStartBtn: String
    val dlStartNowBtn: String
    val dlOpenFile: String
    val dlOpenFolder: String
    val dlStatusStarting: String
    val dlStatusFinished: String
    val dlStatusError: String
    val dlDepsRequired: String
    val dlDepsInstall: String
    val dlDepsReady: String
    val dlDepsNotInstalled: String
    val dlDepsDownloading: String
    val dlDepsExtracting: String
    val dlDepsChecking: String
    val dlCancel: String
    val dlChangeFolder: String
    val dlDeleteDeps: String
}

interface MobileDownloadOptionsText {
    val dlAdvanced: String
    val dlOptMetadata: String
    val dlOptSponsorblock: String
    val dlOptSubtitles: String
    val dlOptPlaylist: String
    val dlVideoLabel: String
    val dlAudioLabel: String
    val dlQualityLabel: String
    val dlSubtitleLabel: String
    val dlBest: String
    val dlAuto: String
    val dlScanning: String
    val dlSubsFoundHeader: String
    val dlSubsNoneFound: String
    val dlRetry: String
    val dlShowLog: String
    val dlHideLog: String
    val dlReset: String
}

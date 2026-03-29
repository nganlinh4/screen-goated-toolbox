package dev.screengoated.toolbox.mobile

import android.content.Context
import dev.screengoated.toolbox.mobile.downloader.DownloaderPersistence
import dev.screengoated.toolbox.mobile.downloader.DownloaderRepository
import dev.screengoated.toolbox.mobile.helpassistant.HelpAssistantClient
import dev.screengoated.toolbox.mobile.bilingualrelay.BilingualRelayRepository
import dev.screengoated.toolbox.mobile.bilingualrelay.BilingualRelayRuntime
import dev.screengoated.toolbox.mobile.history.HistoryBackedPresetHistoryRecorder
import dev.screengoated.toolbox.mobile.history.HistoryPersistence
import dev.screengoated.toolbox.mobile.history.HistoryRepository
import dev.screengoated.toolbox.mobile.preset.AudioApiClient
import dev.screengoated.toolbox.mobile.preset.AudioPresetLaunchStore
import dev.screengoated.toolbox.mobile.preset.ApiKeys
import dev.screengoated.toolbox.mobile.preset.PresetPersistence
import dev.screengoated.toolbox.mobile.preset.PresetRepository
import dev.screengoated.toolbox.mobile.preset.TextApiClient
import dev.screengoated.toolbox.mobile.preset.VisionApiClient
import dev.screengoated.toolbox.mobile.model.AndroidLiveSessionRepository
import dev.screengoated.toolbox.mobile.model.PermissionSnapshotEvaluator
import dev.screengoated.toolbox.mobile.service.GeminiLiveSocketClient
import dev.screengoated.toolbox.mobile.service.RealtimeTranslationClient
import dev.screengoated.toolbox.mobile.service.parakeet.ParakeetModelManager
import dev.screengoated.toolbox.mobile.service.tts.AndroidTtsRuntimeService
import dev.screengoated.toolbox.mobile.service.tts.EdgeVoiceCatalogService
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore
import dev.screengoated.toolbox.mobile.storage.SecureSettingsStore
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionStore
import dev.screengoated.toolbox.mobile.updater.AppUpdateRepository
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import java.util.concurrent.TimeUnit

class AppContainer(
    context: Context,
) {
    private val appContext = context.applicationContext

    val json: Json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    val httpClient: OkHttpClient = OkHttpClient.Builder()
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .writeTimeout(30, TimeUnit.SECONDS)
        .build()

    private val helpAssistantHttpClient: OkHttpClient = httpClient.newBuilder()
        .connectTimeout(30, TimeUnit.SECONDS)
        .readTimeout(15, TimeUnit.MINUTES)
        .writeTimeout(15, TimeUnit.MINUTES)
        .callTimeout(0, TimeUnit.MILLISECONDS)
        .build()

    val projectionConsentStore = ProjectionConsentStore()
    val audioPresetLaunchStore = AudioPresetLaunchStore()
    private val settingsStore = SecureSettingsStore(appContext, json)
    private val permissionEvaluator = PermissionSnapshotEvaluator(projectionConsentStore)
    private val sessionStore = LiveSessionStore()
    private val edgeVoiceCatalogService = EdgeVoiceCatalogService(httpClient, settingsStore, json)
    private val historyPersistence = HistoryPersistence(appContext, json)
    val historyRepository = HistoryRepository(historyPersistence)

    val repository = AndroidLiveSessionRepository(
        context = appContext,
        store = sessionStore,
        settingsStore = settingsStore,
        permissionEvaluator = permissionEvaluator,
        projectionConsentStore = projectionConsentStore,
        overlaySupported = BuildConfig.OVERLAY_SUPPORTED,
        historyRepository = historyRepository,
    )
    val bilingualRelayRepository = BilingualRelayRepository(settingsStore)

    val parakeetModelManager = ParakeetModelManager(appContext)

    private val downloaderPersistence = DownloaderPersistence(appContext, json)
    val downloaderRepository = DownloaderRepository(appContext, downloaderPersistence).also {
        it.checkTools() // Check tool status on app startup so Settings UI shows correct state
    }
    val appUpdateRepository = AppUpdateRepository(httpClient)

    private val textApiClient = TextApiClient(httpClient)
    val helpAssistantClient = HelpAssistantClient(helpAssistantHttpClient)
    val audioApiClient = AudioApiClient(
        appContext = appContext,
        httpClient = httpClient,
        parakeetModelManager = parakeetModelManager,
    )
    private val visionApiClient = VisionApiClient(httpClient)
    private val presetPersistence = PresetPersistence(appContext, json)
    val presetRepository = PresetRepository(
        textApiClient = textApiClient,
        audioApiClient = audioApiClient,
        visionApiClient = visionApiClient,
        apiKeys = {
            ApiKeys(
                geminiKey = repository.currentApiKey(),
                cerebrasKey = repository.currentCerebrasApiKey(),
                groqKey = repository.currentGroqApiKey(),
                openRouterKey = repository.currentOpenRouterApiKey(),
                ollamaBaseUrl = repository.currentOllamaUrl(),
            )
        },
        runtimeSettings = { settingsStore.loadPresetRuntimeSettings() },
        uiLanguage = { repository.currentUiPreferences().uiLanguage },
        overrideStore = presetPersistence,
        historyRecorder = HistoryBackedPresetHistoryRecorder(historyRepository),
    )

    val geminiLiveSocketClient = GeminiLiveSocketClient(httpClient)
    val realtimeTranslationClient = RealtimeTranslationClient(httpClient)
    val bilingualRelayRuntime = BilingualRelayRuntime(
        context = appContext,
        projectionConsentStore = projectionConsentStore,
        repository = bilingualRelayRepository,
        httpClient = httpClient,
    )
    val ttsRuntimeService = AndroidTtsRuntimeService(
        context = appContext,
        httpClient = httpClient,
        settingsStore = settingsStore,
        edgeVoiceCatalogService = edgeVoiceCatalogService,
    )
}

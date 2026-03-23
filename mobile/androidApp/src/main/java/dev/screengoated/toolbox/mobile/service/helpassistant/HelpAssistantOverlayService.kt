package dev.screengoated.toolbox.mobile.service.helpassistant

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.Rect
import android.os.Build
import android.os.IBinder
import android.provider.Settings
import android.view.WindowManager
import android.widget.Toast
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.helpassistant.HelpAssistantBucket
import dev.screengoated.toolbox.mobile.helpassistant.HelpAssistantRequest
import dev.screengoated.toolbox.mobile.helpassistant.errorMarkdown
import dev.screengoated.toolbox.mobile.helpassistant.helpAssistantBucketFromWireId
import dev.screengoated.toolbox.mobile.helpassistant.label
import dev.screengoated.toolbox.mobile.helpassistant.loadingMessage
import dev.screengoated.toolbox.mobile.helpassistant.resultMarkdown
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import dev.screengoated.toolbox.mobile.service.preset.PresetButtonCanvasHtmlBuilder
import dev.screengoated.toolbox.mobile.service.preset.PresetMarkdownRenderer
import dev.screengoated.toolbox.mobile.service.preset.PresetOverlayDismissTarget
import dev.screengoated.toolbox.mobile.service.preset.PresetOverlayResultModule
import dev.screengoated.toolbox.mobile.service.preset.PresetResultHtmlBuilder
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch

class HelpAssistantOverlayService : Service() {
    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private lateinit var windowManager: WindowManager
    private lateinit var resultModule: PresetOverlayResultModule

    private var uiPreferencesJob: Job? = null
    private var requestJob: Job? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        if (!Settings.canDrawOverlays(this)) {
            Toast.makeText(this, "Overlay permission is required for Help Assistant.", Toast.LENGTH_SHORT).show()
            stopSelf()
            return
        }

        windowManager = getSystemService(WINDOW_SERVICE) as WindowManager
        val appContainer = (application as SgtMobileApplication).appContainer
        resultModule = PresetOverlayResultModule(
            context = this,
            windowManager = windowManager,
            presetRepository = appContainer.presetRepository,
            dismissTarget = PresetOverlayDismissTarget(this, windowManager, ::uiLanguage),
            resultHtmlBuilder = PresetResultHtmlBuilder(this),
            buttonCanvasHtmlBuilder = PresetButtonCanvasHtmlBuilder(this),
            renderer = PresetMarkdownRenderer(this),
            uiLanguage = ::uiLanguage,
            isDarkTheme = ::isDarkTheme,
            screenBoundsProvider = ::screenBounds,
            dp = ::dp,
            cssToPhysical = ::cssToPhysical,
            onRequestInputFront = {},
            onDismissAll = { stopSelf() },
            onNoOverlaysRemaining = { stopSelf() },
            onMicRequested = {},
            overlayOpacityProvider = {
                appContainer.repository.currentUiPreferences().overlayOpacityPercent.coerceIn(10, 100)
            },
        )

        uiPreferencesJob = serviceScope.launch {
            appContainer.repository.uiPreferences.collectLatest {
                resultModule.refreshResultWindowsForTheme()
                resultModule.refreshCanvasWindowForPreferences()
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val startIntent = intent ?: return START_NOT_STICKY
        val bucket = helpAssistantBucketFromWireId(startIntent.getStringExtra(EXTRA_BUCKET))
            ?: return START_NOT_STICKY
        val question = startIntent.getStringExtra(EXTRA_QUESTION)?.trim().orEmpty()
        if (question.isBlank()) {
            return START_NOT_STICKY
        }
        val uiLanguage = startIntent.getStringExtra(EXTRA_UI_LANGUAGE).orEmpty().ifBlank { uiLanguage() }
        renderLoading(bucket, uiLanguage)
        launchRequest(bucket, question, uiLanguage)
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        super.onDestroy()
        requestJob?.cancel()
        uiPreferencesJob?.cancel()
        resultModule.destroy()
        serviceScope.cancel()
    }

    private fun launchRequest(
        bucket: HelpAssistantBucket,
        question: String,
        uiLanguage: String,
    ) {
        val appContainer = (application as SgtMobileApplication).appContainer
        requestJob?.cancel()
        requestJob = serviceScope.launch {
            val result = appContainer.helpAssistantClient.ask(
                HelpAssistantRequest(
                    bucket = bucket,
                    question = question,
                    uiLanguage = uiLanguage,
                    geminiApiKey = appContainer.repository.currentApiKey(),
                ),
            )
            val locale = MobileLocaleText.forLanguage(uiLanguage)
            val markdown = result.fold(
                onSuccess = { answer -> bucket.resultMarkdown(locale, question, answer) },
                onFailure = { error -> bucket.errorMarkdown(error.message ?: "Unknown error") },
            )
            val isError = result.isFailure
            val windowState = PresetResultWindowState(
                id = WINDOW_ID,
                blockIdx = 0,
                title = bucket.label(locale),
                markdownText = markdown,
                isLoading = false,
                isStreaming = false,
                isError = isError,
                renderMode = "markdown",
                overlayOrder = 0,
            )
            resultModule.showStandaloneMarkdownWindow(windowState)
        }
    }

    private fun renderLoading(
        bucket: HelpAssistantBucket,
        uiLanguage: String,
    ) {
        val locale = MobileLocaleText.forLanguage(uiLanguage)
        resultModule.showStandaloneMarkdownWindow(
            PresetResultWindowState(
                id = WINDOW_ID,
                blockIdx = 0,
                title = bucket.label(locale),
                markdownText = bucket.loadingMessage(locale),
                isLoading = true,
                loadingStatusText = bucket.loadingMessage(locale),
                isStreaming = false,
                isError = false,
                renderMode = "markdown",
                overlayOrder = 0,
            ),
        )
    }

    private fun uiLanguage(): String {
        val appContainer = (application as SgtMobileApplication).appContainer
        return appContainer.repository.currentUiPreferences().uiLanguage
    }

    private fun isDarkTheme(): Boolean {
        val appContainer = (application as SgtMobileApplication).appContainer
        return when (appContainer.repository.currentUiPreferences().themeMode) {
            MobileThemeMode.DARK -> true
            MobileThemeMode.LIGHT -> false
            MobileThemeMode.SYSTEM -> {
                val nightModeFlags = resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
                nightModeFlags == Configuration.UI_MODE_NIGHT_YES
            }
        }
    }

    private fun screenBounds(): Rect {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val metrics = windowManager.currentWindowMetrics.bounds
            Rect(0, 0, metrics.width(), metrics.height())
        } else {
            val metrics = resources.displayMetrics
            Rect(0, 0, metrics.widthPixels, metrics.heightPixels)
        }
    }

    private fun dp(value: Int): Int =
        (value * resources.displayMetrics.density).toInt()

    private fun cssToPhysical(value: Int): Int =
        (value * resources.displayMetrics.density).toInt()

    companion object {
        private const val EXTRA_BUCKET = "bucket"
        private const val EXTRA_QUESTION = "question"
        private const val EXTRA_UI_LANGUAGE = "ui_language"
        private const val SESSION_ID = "help-assistant"
        private val WINDOW_ID = PresetResultWindowId(sessionId = SESSION_ID, blockIdx = 0)

        fun start(
            context: Context,
            bucket: HelpAssistantBucket,
            question: String,
            uiLanguage: String,
        ) {
            context.startService(
                Intent(context, HelpAssistantOverlayService::class.java)
                    .putExtra(EXTRA_BUCKET, bucket.wireId)
                    .putExtra(EXTRA_QUESTION, question)
                    .putExtra(EXTRA_UI_LANGUAGE, uiLanguage),
            )
        }
    }
}

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
import androidx.core.net.toUri
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.helpassistant.HelpAssistantRequest
import dev.screengoated.toolbox.mobile.helpassistant.HelpAssistantPendingLaunchStore
import dev.screengoated.toolbox.mobile.helpassistant.helpErrorMarkdown
import dev.screengoated.toolbox.mobile.helpassistant.helpLoadingMessage
import dev.screengoated.toolbox.mobile.helpassistant.helpResultMarkdown
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import dev.screengoated.toolbox.mobile.service.preset.PresetButtonCanvasHtmlBuilder
import dev.screengoated.toolbox.mobile.service.preset.PresetMarkdownRenderer
import dev.screengoated.toolbox.mobile.service.DismissBubbleController
import dev.screengoated.toolbox.mobile.service.dismissAllLabel
import dev.screengoated.toolbox.mobile.service.preset.PresetOverlayResultModule
import dev.screengoated.toolbox.mobile.service.preset.PresetResultHtmlBuilder
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText
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
    private var resultModule: PresetOverlayResultModule? = null

    private var uiPreferencesJob: Job? = null
    private var requestJob: Job? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        windowManager = getSystemService(WINDOW_SERVICE) as WindowManager
        val appContainer = (application as SgtMobileApplication).appContainer
        resultModule = PresetOverlayResultModule(
            context = this,
            windowManager = windowManager,
            presetRepository = appContainer.presetRepository,
            dismissTarget = DismissBubbleController(
                context = this,
                windowManager = windowManager,
                showDismissAll = true,
                allLabel = { dismissAllLabel(uiLanguage()) },
            ),
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
                resultModule?.refreshResultWindowsForTheme()
                resultModule?.refreshCanvasWindowForPreferences()
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val startIntent = intent ?: return START_NOT_STICKY
        val question = startIntent.getStringExtra(EXTRA_QUESTION)?.trim().orEmpty()
        if (question.isBlank()) return START_NOT_STICKY
        val uiLanguage = startIntent.getStringExtra(EXTRA_UI_LANGUAGE).orEmpty().ifBlank { uiLanguage() }
        if (!Settings.canDrawOverlays(this)) {
            HelpAssistantPendingLaunchStore.set(question, uiLanguage)
            Toast.makeText(this, getString(R.string.help_assistant_overlay_permission_required), Toast.LENGTH_SHORT).show()
            val permissionIntent = Intent(
                Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                "package:$packageName".toUri(),
            ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            startActivity(permissionIntent)
            stopSelf()
            return START_NOT_STICKY
        }
        renderLoading(uiLanguage)
        launchRequest(question, uiLanguage)
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        super.onDestroy()
        requestJob?.cancel()
        uiPreferencesJob?.cancel()
        resultModule?.destroy()
        serviceScope.cancel()
    }

    private fun launchRequest(question: String, uiLanguage: String) {
        val appContainer = (application as SgtMobileApplication).appContainer
        requestJob?.cancel()
        requestJob = serviceScope.launch {
            val result = appContainer.helpAssistantClient.ask(
                HelpAssistantRequest(
                    question = question,
                    uiLanguage = uiLanguage,
                    geminiApiKey = appContainer.repository.currentApiKey(),
                ),
            )
            result.exceptionOrNull()?.let { error ->
                apiKeyErrorToastText(error.message ?: error.toString(), uiLanguage)?.let(appContainer.toastBus::show)
            }
            val markdown = result.fold(
                onSuccess = { answer -> helpResultMarkdown(question, answer) },
                onFailure = { error ->
                    val rawMessage = error.message ?: "Unknown error"
                    helpErrorMarkdown(apiKeyErrorToastText(rawMessage, uiLanguage) ?: rawMessage)
                },
            )
            val isError = result.isFailure
            val windowState = PresetResultWindowState(
                id = WINDOW_ID,
                blockIdx = 0,
                title = "Ask SGT",
                markdownText = markdown,
                isLoading = false,
                isStreaming = false,
                isError = isError,
                renderMode = "markdown",
                overlayOrder = 0,
            )
            resultModule?.showStandaloneMarkdownWindow(windowState)
        }
    }

    private fun renderLoading(uiLanguage: String) {
        val locale = MobileLocaleText.forLanguage(uiLanguage)
        val msg = helpLoadingMessage(locale)
        resultModule?.showStandaloneMarkdownWindow(
            PresetResultWindowState(
                id = WINDOW_ID,
                blockIdx = 0,
                title = "Ask SGT",
                markdownText = msg,
                isLoading = true,
                loadingStatusText = msg,
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
        private const val EXTRA_QUESTION = "question"
        private const val EXTRA_UI_LANGUAGE = "ui_language"
        private const val SESSION_ID = "help-assistant"
        private val WINDOW_ID = PresetResultWindowId(sessionId = SESSION_ID, blockIdx = 0)

        fun start(context: Context, question: String, uiLanguage: String) {
            context.startService(
                Intent(context, HelpAssistantOverlayService::class.java)
                    .putExtra(EXTRA_QUESTION, question)
                    .putExtra(EXTRA_UI_LANGUAGE, uiLanguage),
            )
        }
    }
}

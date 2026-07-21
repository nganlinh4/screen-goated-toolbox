package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.lifecycle.lifecycleScope
import dev.screengoated.toolbox.mobile.BuildConfig
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

internal interface CreationPickerHost {
    fun pickImages(requestId: String)
    fun pickOutputDirectory(requestId: String)
    fun closeMiniApp()
    fun minimizeMiniApp()
}

class CreationMiniAppActivity : ComponentActivity(), CreationPickerHost {
    private lateinit var tool: CreationTool
    private lateinit var webView: WebView
    private lateinit var bridge: CreationWebBridge
    private lateinit var manager: CreationJobManager
    private var pendingImageRequest: String? = null
    private var pendingDirectoryRequest: String? = null

    private val imagePicker = registerForActivityResult(
        ActivityResultContracts.OpenMultipleDocuments(),
    ) { uris ->
        val requestId = pendingImageRequest ?: return@registerForActivityResult
        pendingImageRequest = null
        lifecycleScope.launch {
            val paths = withContext(Dispatchers.IO) { manager.files.importImages(uris) }
            bridge.resolvePicker(requestId, paths)
        }
    }

    private val outputPicker = registerForActivityResult(
        ActivityResultContracts.OpenDocumentTree(),
    ) { uri ->
        val requestId = pendingDirectoryRequest ?: return@registerForActivityResult
        pendingDirectoryRequest = null
        val label = uri?.let(manager.files::rememberOutputDirectory)
        bridge.resolvePicker(requestId, label)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        tool = CreationTool.fromWireName(intent.getStringExtra(EXTRA_TOOL)) ?: run {
            finish()
            return
        }
        enableEdgeToEdge()
        manager = CreationJobManager.get(this)
        manager.startPreparation(tool)
        val contextJson = hostContextJson()
        webView = WebView(this).apply {
            setBackgroundColor(Color.TRANSPARENT)
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            settings.databaseEnabled = true
            settings.allowFileAccess = true
            settings.allowContentAccess = true
            settings.mediaPlaybackRequiresUserGesture = false
            settings.userAgentString = "${settings.userAgentString} SGT-Mobile/${BuildConfig.VERSION_NAME}"
            addJavascriptInterface(
                CreationWebBridge(
                    host = this@CreationMiniAppActivity,
                    tool = tool,
                    webView = this,
                    manager = manager,
                    contextJson = contextJson,
                ).also { bridge = it },
                "CreationBridge",
            )
            webViewClient = object : WebViewClient() {
                override fun shouldInterceptRequest(
                    view: WebView?,
                    request: WebResourceRequest?,
                ): WebResourceResponse? = request?.url?.let(manager.assets::intercept)
                    ?: super.shouldInterceptRequest(view, request)

                override fun onRenderProcessGone(
                    view: WebView?,
                    detail: RenderProcessGoneDetail?,
                ): Boolean {
                    finish()
                    return true
                }
            }
            loadUrl(
                "${CreationAssetRegistry.CREATION_ORIGIN}/${tool.assetDirectory}/index.html",
            )
        }
        WebView.setWebContentsDebuggingEnabled(BuildConfig.DEBUG)
        setContentView(webView)
        onBackPressedDispatcher.addCallback(this, object : androidx.activity.OnBackPressedCallback(true) {
            override fun handleOnBackPressed() = finish()
        })
    }

    override fun pickImages(requestId: String) {
        if (pendingImageRequest != null) {
            bridge.rejectPicker(requestId, "An image picker is already open")
            return
        }
        pendingImageRequest = requestId
        imagePicker.launch(arrayOf("image/png", "image/jpeg", "image/webp"))
    }

    override fun pickOutputDirectory(requestId: String) {
        if (pendingDirectoryRequest != null) {
            bridge.rejectPicker(requestId, "A folder picker is already open")
            return
        }
        pendingDirectoryRequest = requestId
        outputPicker.launch(null)
    }

    override fun closeMiniApp() = finish()

    override fun minimizeMiniApp() {
        moveTaskToBack(false)
    }

    override fun onDestroy() {
        if (::bridge.isInitialized) bridge.destroy()
        if (::webView.isInitialized) {
            webView.removeJavascriptInterface("CreationBridge")
            webView.stopLoading()
            webView.destroy()
        }
        super.onDestroy()
    }

    private fun hostContextJson(): String {
        val preferences = (application as SgtMobileApplication).appContainer.repository
            .currentUiPreferences()
        val systemDark = resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK ==
            Configuration.UI_MODE_NIGHT_YES
        val dark = when (preferences.themeMode) {
            MobileThemeMode.SYSTEM -> systemDark
            MobileThemeMode.DARK -> true
            MobileThemeMode.LIGHT -> false
        }
        return JSONObject()
            .put("language", preferences.uiLanguage)
            .put("theme", if (dark) "dark" else "light")
            .toString()
    }

    companion object {
        private const val EXTRA_TOOL = "creation_tool"

        internal fun intent(context: Context, tool: CreationTool): Intent = Intent(
            context,
            CreationMiniAppActivity::class.java,
        ).putExtra(EXTRA_TOOL, tool.wireName)
    }
}

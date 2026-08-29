package dev.screengoated.toolbox.mobile.creation

import android.graphics.Color as AndroidColor
import android.webkit.WebSettings
import android.webkit.WebView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import dev.screengoated.toolbox.mobile.ui.i18n.Creation3dLocale
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
internal fun CreationModelViewer(
    outputPath: String,
    segmented: Boolean,
    viewModel: CreationNativeViewModel,
    strings: Creation3dLocale,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val webViewAvailable = remember {
        runCatching { WebView.getCurrentWebViewPackage()?.packageName }
            .getOrNull()
            .let(::supportsCreationWebViewer)
    }
    val previewFile by produceState<Result<File>?>(null, outputPath, webViewAvailable) {
        value = null
        if (!webViewAvailable) return@produceState
        value = withContext(Dispatchers.IO) {
            runCatching { viewModel.viewerModelFile(outputPath) }
        }
    }
    val currentFile = previewFile?.getOrNull()
    DisposableEffect(currentFile) {
        onDispose { currentFile?.let(viewModel::releaseViewerModelFile) }
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.surfaceContainerLowest),
        contentAlignment = Alignment.Center,
    ) {
        when {
            !webViewAvailable -> ViewerUnavailable(strings)
            previewFile == null -> CircularProgressIndicator()
            previewFile?.isFailure == true -> ViewerUnavailable(strings)
            currentFile != null -> CreationModelWebSurface(
                modelFile = currentFile,
                segmented = segmented,
                strings = strings,
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

@Composable
private fun ViewerUnavailable(strings: Creation3dLocale) {
    Text(
        text = strings.previewUnavailable,
        color = MaterialTheme.colorScheme.error,
    )
}

@Composable
private fun CreationModelWebSurface(
    modelFile: File,
    segmented: Boolean,
    strings: Creation3dLocale,
    modifier: Modifier,
) {
    val context = LocalContext.current
    val darkTheme = !MaterialTheme.colorScheme.surfaceContainerLowest.isLightColor()
    val session = remember(modelFile, segmented, darkTheme, strings) {
        CreationModelViewerSession.create(modelFile, segmented, darkTheme, strings)
    }
    val webView = remember(session) {
        runCatching {
            WebView(context).apply {
                setBackgroundColor(AndroidColor.TRANSPARENT)
                settings.apply {
                    javaScriptEnabled = true
                    javaScriptCanOpenWindowsAutomatically = false
                    domStorageEnabled = false
                    allowFileAccess = false
                    allowContentAccess = false
                    blockNetworkLoads = true
                    mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
                    cacheMode = WebSettings.LOAD_NO_CACHE
                    builtInZoomControls = false
                    displayZoomControls = false
                    setSupportMultipleWindows(false)
                    setGeolocationEnabled(false)
                    mediaPlaybackRequiresUserGesture = true
                    safeBrowsingEnabled = true
                }
                removeJavascriptInterface("searchBoxJavaBridge_")
                removeJavascriptInterface("accessibility")
                removeJavascriptInterface("accessibilityTraversal")
                webViewClient = CreationModelViewerClient(context.assets, session)
                setOnTouchListener { view, event ->
                    when (event.actionMasked) {
                        android.view.MotionEvent.ACTION_DOWN,
                        android.view.MotionEvent.ACTION_POINTER_DOWN,
                        android.view.MotionEvent.ACTION_MOVE,
                        -> view.parent?.requestDisallowInterceptTouchEvent(true)
                        android.view.MotionEvent.ACTION_UP,
                        android.view.MotionEvent.ACTION_CANCEL,
                        -> view.parent?.requestDisallowInterceptTouchEvent(false)
                    }
                    false
                }
                loadUrl(session.documentUrl)
            }
        }
    }
    val view = webView.getOrNull()
    if (view == null) {
        ViewerUnavailable(strings)
        return
    }
    DisposableEffect(view) {
        onDispose {
            view.evaluateJavascript("window.sgtModelViewer?.dispose()", null)
            view.stopLoading()
            view.loadUrl("about:blank")
            view.clearHistory()
            view.removeAllViews()
            view.destroy()
        }
    }
    AndroidView(
        factory = { view },
        modifier = modifier,
    )
}

private fun androidx.compose.ui.graphics.Color.isLightColor(): Boolean =
    (0.2126f * red + 0.7152f * green + 0.0722f * blue) > 0.5f

internal fun supportsCreationWebViewer(webViewPackageName: String?): Boolean =
    !webViewPackageName.isNullOrBlank()

@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.PixelFormat
import android.os.Build
import android.view.Gravity
import android.view.WindowManager
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.LoadingIndicator
import androidx.compose.material3.LoadingIndicatorDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.unit.dp
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner

/** Minimal lifecycle owner for hosting ComposeView in a service overlay. */
private class OverlayLifecycleOwner : androidx.lifecycle.LifecycleOwner,
    androidx.savedstate.SavedStateRegistryOwner {
    private val lifecycleRegistry = androidx.lifecycle.LifecycleRegistry(this)
    private val savedStateRegistryController = androidx.savedstate.SavedStateRegistryController.create(this)

    init {
        savedStateRegistryController.performRestore(null)
    }

    override val lifecycle: androidx.lifecycle.Lifecycle get() = lifecycleRegistry
    override val savedStateRegistry: androidx.savedstate.SavedStateRegistry
        get() = savedStateRegistryController.savedStateRegistry

    fun handleLifecycleEvent(event: androidx.lifecycle.Lifecycle.Event) {
        lifecycleRegistry.handleLifecycleEvent(event)
    }
}

/**
 * A transparent click-through overlay that shows a M3E LoadingIndicator
 * at the center of the screen while a preset is executing.
 * Tinted with the preset's category accent color.
 */
internal class PresetProcessingIndicator(
    private val context: Context,
    private val windowManager: WindowManager,
) {
    private var composeView: ComposeView? = null
    private var lifecycleOwner: OverlayLifecycleOwner? = null

    fun show(accentColor: Color = Color(0xFF5C9CE6)) {
        if (composeView != null) return

        val lifecycleOwner = OverlayLifecycleOwner()
        val view = ComposeView(context).apply {
            setViewTreeLifecycleOwner(lifecycleOwner)
            setViewTreeSavedStateRegistryOwner(lifecycleOwner)
            setContent {
                ProcessingIndicatorContent(accentColor)
            }
        }
        lifecycleOwner.handleLifecycleEvent(androidx.lifecycle.Lifecycle.Event.ON_CREATE)
        lifecycleOwner.handleLifecycleEvent(androidx.lifecycle.Lifecycle.Event.ON_START)
        lifecycleOwner.handleLifecycleEvent(androidx.lifecycle.Lifecycle.Event.ON_RESUME)

        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.WRAP_CONTENT,
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O)
                WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY
            else @Suppress("DEPRECATION") WindowManager.LayoutParams.TYPE_PHONE,
            // Transparent to input (click-through) + not focusable
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.CENTER
            val sizePx = (160 * context.resources.displayMetrics.density).toInt()
            width = sizePx
            height = sizePx
        }

        try {
            windowManager.addView(view, params)
            composeView = view
            this.lifecycleOwner = lifecycleOwner
        } catch (_: Exception) {}
    }

    fun dismiss() {
        lifecycleOwner?.handleLifecycleEvent(androidx.lifecycle.Lifecycle.Event.ON_DESTROY)
        lifecycleOwner = null
        composeView?.let {
            try { windowManager.removeView(it) } catch (_: Exception) {}
        }
        composeView = null
    }

    val isShowing: Boolean get() = composeView != null
}

@Composable
private fun ProcessingIndicatorContent(accentColor: Color) {
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier.size(160.dp),
    ) {
        LoadingIndicator(
            color = accentColor.copy(alpha = 1f),
            modifier = Modifier.size(80.dp),
        )
    }
}

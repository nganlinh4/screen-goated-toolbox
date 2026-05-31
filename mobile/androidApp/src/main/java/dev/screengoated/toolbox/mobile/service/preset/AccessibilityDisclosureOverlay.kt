@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.PixelFormat
import android.view.Gravity
import android.view.WindowManager
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.unit.dp
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.ui.theme.SgtMobileTheme

/** Minimal lifecycle owner for hosting an interactive ComposeView in a service overlay. */
private class DisclosureLifecycleOwner : androidx.lifecycle.LifecycleOwner,
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

/** Localized copy shown in the accessibility prominent-disclosure dialog. */
internal data class AccessibilityDisclosureStrings(
    val title: String,
    val body: String,
    val agree: String,
    val cancel: String,
)

/**
 * Prominent disclosure consent dialog shown BEFORE the app sends the user to
 * the system Accessibility settings. Required by Google Play because the app
 * uses the AccessibilityService API for app functionality (not as an
 * accessibility tool) and reads sensitive user data (selected text, clipboard,
 * screen content). Rendered as a focusable, dimmed full-screen overlay so it
 * can be shown from the foreground service context.
 */
internal class AccessibilityDisclosureOverlay(
    private val context: Context,
    private val windowManager: WindowManager,
) {
    private var composeView: ComposeView? = null
    private var lifecycleOwner: DisclosureLifecycleOwner? = null

    fun show(
        themeMode: MobileThemeMode,
        strings: AccessibilityDisclosureStrings,
        onAgree: () -> Unit,
    ) {
        if (composeView != null) return

        val owner = DisclosureLifecycleOwner()
        val view = ComposeView(context).apply {
            setViewTreeLifecycleOwner(owner)
            setViewTreeSavedStateRegistryOwner(owner)
            setContent {
                SgtMobileTheme(themeMode = themeMode) {
                    DisclosureContent(
                        strings = strings,
                        onAgree = { dismiss(); onAgree() },
                        onCancel = { dismiss() },
                    )
                }
            }
        }
        owner.handleLifecycleEvent(androidx.lifecycle.Lifecycle.Event.ON_CREATE)
        owner.handleLifecycleEvent(androidx.lifecycle.Lifecycle.Event.ON_START)
        owner.handleLifecycleEvent(androidx.lifecycle.Lifecycle.Event.ON_RESUME)

        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            // Focusable + touchable (interactive), and dim the content behind it.
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_DIM_BEHIND,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.CENTER
            dimAmount = 0.55f
        }

        try {
            windowManager.addView(view, params)
            composeView = view
            lifecycleOwner = owner
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
private fun DisclosureContent(
    strings: AccessibilityDisclosureStrings,
    onAgree: () -> Unit,
    onCancel: () -> Unit,
) {
    Box(
        modifier = Modifier.fillMaxSize(),
        contentAlignment = Alignment.Center,
    ) {
        Surface(
            modifier = Modifier
                .padding(28.dp)
                .widthIn(max = 400.dp),
            shape = MaterialTheme.shapes.extraLarge,
            color = MaterialTheme.colorScheme.surfaceContainerHigh,
            tonalElevation = 6.dp,
        ) {
            Column(modifier = Modifier.padding(28.dp)) {
                Text(
                    text = strings.title,
                    style = MaterialTheme.typography.headlineSmall,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Spacer(Modifier.height(16.dp))
                Text(
                    text = strings.body,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(28.dp))
                Button(
                    onClick = onAgree,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(strings.agree, maxLines = 1, softWrap = false)
                }
                Spacer(Modifier.height(8.dp))
                TextButton(
                    onClick = onCancel,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(strings.cancel, maxLines = 1, softWrap = false)
                }
            }
        }
    }
}

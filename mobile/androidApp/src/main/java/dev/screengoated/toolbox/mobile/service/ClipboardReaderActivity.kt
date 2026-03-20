package dev.screengoated.toolbox.mobile.service

import android.app.Activity
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log

/**
 * Transparent Activity that reads clipboard. Required on Android 10+
 * because only foreground Activities can access clipboard.
 *
 * Uses Theme.Translucent.NoTitleBar — brief visual flash is unavoidable
 * but minimized by finishing as fast as possible.
 */
class ClipboardReaderActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Attempt read immediately in onCreate
        if (tryRead()) {
            finish()
            return
        }
        // Fallback: wait for focus
        Handler(Looper.getMainLooper()).postDelayed({
            if (!isFinishing) {
                tryRead()
                finish()
            }
        }, 500)
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus || isFinishing) return
        if (tryRead()) {
            finish()
        }
    }

    private fun tryRead(): Boolean {
        try {
            val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return false
            val clip = cm.primaryClip ?: return false
            if (clip.itemCount == 0) return false
            val text = clip.getItemAt(0)?.text?.toString()
            if (text.isNullOrBlank()) return false
            lastReadText = text
            lastReadTimestamp = System.currentTimeMillis()
            Log.d("ClipboardReader", "Read: '${text.take(60)}'")
            onClipboardRead?.invoke(text)
            onClipboardRead = null
            return true
        } catch (_: Exception) {
            return false
        }
    }

    companion object {
        @Volatile var lastReadText: String? = null
        @Volatile var lastReadTimestamp: Long = 0
        @Volatile var onClipboardRead: ((String) -> Unit)? = null

        fun launch(context: Context, onRead: ((String) -> Unit)? = null) {
            onClipboardRead = onRead
            context.startActivity(
                Intent(context, ClipboardReaderActivity::class.java)
                    .addFlags(
                        Intent.FLAG_ACTIVITY_NEW_TASK or
                            Intent.FLAG_ACTIVITY_CLEAR_TOP or
                            Intent.FLAG_ACTIVITY_NO_ANIMATION or
                            Intent.FLAG_ACTIVITY_NO_HISTORY,
                    ),
            )
        }

        fun getRecentText(): String? {
            if (System.currentTimeMillis() - lastReadTimestamp > 5000) return null
            return lastReadText
        }

        fun consume() { lastReadText = null }
    }
}

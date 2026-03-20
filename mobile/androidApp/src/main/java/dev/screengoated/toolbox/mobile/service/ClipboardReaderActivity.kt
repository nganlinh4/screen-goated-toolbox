package dev.screengoated.toolbox.mobile.service

import android.app.Activity
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.util.Log

/**
 * Invisible Activity that reads the clipboard and returns the result.
 * Required because Android 12+ restricts clipboard access to visible Activities.
 * Launches, reads, stores result in companion, finishes immediately.
 */
class ClipboardReaderActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus) return
        try {
            val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
            val clip = cm?.primaryClip
            if (clip != null && clip.itemCount > 0) {
                val text = clip.getItemAt(0)?.text?.toString()
                if (!text.isNullOrBlank()) {
                    lastReadText = text
                    lastReadTimestamp = System.currentTimeMillis()
                    Log.d("ClipboardReader", "Read clipboard: '${text.take(80)}'")
                    onClipboardRead?.invoke(text)
                    onClipboardRead = null
                }
            } else {
                Log.d("ClipboardReader", "Clipboard empty")
            }
        } catch (e: Exception) {
            Log.e("ClipboardReader", "Failed to read clipboard", e)
        }
        finish()
    }

    companion object {
        @Volatile
        var lastReadText: String? = null
        @Volatile
        var lastReadTimestamp: Long = 0

        /** Callback invoked on main thread when clipboard is read. */
        @Volatile
        var onClipboardRead: ((String) -> Unit)? = null

        fun launch(context: Context, onRead: ((String) -> Unit)? = null) {
            onClipboardRead = onRead
            val intent = Intent(context, ClipboardReaderActivity::class.java)
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
            context.startActivity(intent)
        }

        /** Get clipboard text if read within the last 5 seconds. Does NOT consume. */
        fun getRecentText(): String? {
            val age = System.currentTimeMillis() - lastReadTimestamp
            if (age > 5000) return null
            return lastReadText
        }

        /** Clear cached text after use. */
        fun consume() {
            lastReadText = null
        }
    }
}

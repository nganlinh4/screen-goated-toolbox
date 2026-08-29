package dev.screengoated.toolbox.mobile.creation.worker

import android.app.Application
import android.webkit.WebView

internal object CreationWorkerProcess {
    private const val MARKER = ":sgt_creation_"

    fun isWorkerProcess(): Boolean = Application.getProcessName().contains(MARKER)

    fun configureWebViewDataDirectory() {
        val process = Application.getProcessName()
        if (!process.contains(MARKER)) return
        val suffix = process.substringAfter(':').replace(Regex("[^a-zA-Z0-9_.-]"), "_")
        WebView.setDataDirectorySuffix(suffix)
    }
}

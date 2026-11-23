package dev.screengoated.toolbox.mobile.downloader

import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/** A cancelled launch cannot retain a process that starts later. */
internal class AnalysisControl {
    private val retired = AtomicBoolean(false)
    private val process = AtomicReference<Process?>(null)

    fun isRetired(): Boolean = retired.get()

    fun attach(child: Process): Boolean {
        process.set(child)
        if (retired.get()) {
            child.destroyForcibly()
            return false
        }
        return true
    }

    /** Mark synchronously; call stopOwned on IO to avoid blocking the UI. */
    fun retire() { retired.set(true) }
    fun stopOwned() { process.get()?.destroyForcibly() }
    fun detach(child: Process) { process.compareAndSet(child, null) }
}

internal fun applyAnalysisIfCurrent(
    session: DownloadSessionState,
    sessionId: Int,
    generation: Long,
    url: String,
    control: AnalysisControl,
    transform: (DownloadSessionState) -> DownloadSessionState,
): DownloadSessionState =
    if (session.id == sessionId && session.analysisGeneration == generation && session.inputUrl == url && !control.isRetired()) transform(session) else session

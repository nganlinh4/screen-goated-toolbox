package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import java.util.concurrent.atomic.AtomicBoolean

internal class PhoneControlSetupSessionRuntime(
    private val onBegin: () -> Unit,
    private val onResetRequested: () -> Unit,
    private val onInputResumed: (String) -> Unit,
) {
    val inputGate = PhoneControlSetupSessionGate()
    private val resetRequested = AtomicBoolean(false)

    val inputAdmitted: Boolean
        get() = inputGate.inputAdmitted

    fun begin() {
        if (!inputGate.begin()) return
        onBegin()
        Log.i(TAG, "setup_session_state state=active input_admitted=false")
    }

    fun finish(waitForAnnouncement: Boolean) {
        if (!inputGate.finish(waitForAnnouncement)) return
        resetRequested.set(true)
        onResetRequested()
        Log.i(
            TAG,
            "setup_session_state state=reset_requested input_admitted=false " +
                "announcement_pending=$waitForAnnouncement",
        )
    }

    fun consumeResetRequest(): Boolean = resetRequested.compareAndSet(true, false)

    fun observeFreshSession() {
        observeBoundary(inputGate.observeFreshSession(), "fresh_session")
    }

    fun observeAnnouncementFinished() {
        observeBoundary(inputGate.observeAnnouncementFinished(), "announcement_finished")
    }

    private fun observeBoundary(resumed: Boolean, source: String) {
        if (resumed) {
            onInputResumed(source)
        } else {
            Log.i(TAG, "setup_session_state state=$source input_admitted=false")
        }
    }

    private companion object {
        const val TAG = "SGTPhoneControl"
    }
}

package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log

internal class PhoneControlRuntimeUiGoalSubmission(
    private val queue: PhoneControlUserInterfaceGoalQueue,
    private val runtimeReady: () -> Boolean,
    private val requestScreenRefresh: () -> Unit,
) {
    fun submit(
        text: String,
        presentation: PhoneControlUiGoalPresentation,
        replacePending: Boolean = true,
    ): Long? {
        val result = queue.offer(text, runtimeReady(), presentation, replacePending)
        if (result.disposition == PhoneControlUiGoalOffer.REJECTED) return null
        requestScreenRefresh()
        Log.i(
            TAG,
            "ui_goal_queued id=${result.id} presentation=${presentation.name.lowercase()} " +
                "replaced=${result.disposition == PhoneControlUiGoalOffer.REPLACED}",
        )
        return result.id
    }

    private companion object {
        const val TAG = "SGTPhoneControl"
    }
}

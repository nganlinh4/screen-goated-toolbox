package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Context
import androidx.annotation.StringRes
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntime
import dev.screengoated.toolbox.mobile.service.tts.TtsConsumer
import dev.screengoated.toolbox.mobile.service.tts.TtsPriority
import dev.screengoated.toolbox.mobile.service.tts.TtsRequest
import dev.screengoated.toolbox.mobile.service.tts.TtsRequestMode
import dev.screengoated.toolbox.mobile.service.tts.toRuntimeSnapshot
import dev.screengoated.toolbox.mobile.ui.i18n.uiLocalized
import java.io.Closeable
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch

internal class PhoneControlAuthoritySetupAnnouncer(
    context: Context,
    private val runtime: () -> PhoneControlRuntime?,
) : Closeable {
    private val app = context.applicationContext as SgtMobileApplication
    private val localized = context.uiLocalized()
    private val tts = app.appContainer.ttsRuntimeService
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var startRequestId: Long? = null
    private var successOwner: String? = null
    private var successGeneration = 0L
    private var timeoutJob: Job? = null

    init {
        scope.launch {
            tts.playbackEvents.collect { event ->
                if (event.ownerToken == successOwner) {
                    completeSuccessSpeech(event.completionStatus.name.lowercase())
                }
            }
        }
    }

    fun onSetupSessionEvent(event: PhoneControlAuthoritySetupSessionEvent) {
        when (event) {
            PhoneControlAuthoritySetupSessionEvent.STARTED -> {
                abandonPendingSuccessSpeech()
                runtime()?.beginAuthoritySetupSession()
                startRequestId = speak(
                    R.string.phone_control_setup_voice_start,
                    START_OWNER,
                    "start",
                )
            }
            PhoneControlAuthoritySetupSessionEvent.SUCCEEDED -> {
                runtime()?.finishAuthoritySetupSession(waitForAnnouncement = true)
                val generation = ++successGeneration
                val owner = "$SUCCESS_OWNER_PREFIX$generation"
                successOwner = owner
                val requestId = speak(
                    R.string.phone_control_setup_voice_success,
                    owner,
                    "success",
                )
                if (requestId == null) {
                    completeSuccessSpeech("request_failed")
                } else {
                    timeoutJob?.cancel()
                    timeoutJob = scope.launch {
                        delay(SUCCESS_SPEECH_TIMEOUT_MS)
                        if (successOwner == owner) completeSuccessSpeech("timeout")
                    }
                }
            }
            PhoneControlAuthoritySetupSessionEvent.ENDED -> {
                abandonPendingSuccessSpeech()
                startRequestId?.let(tts::stopIfActive)
                startRequestId = null
                runtime()?.finishAuthoritySetupSession(waitForAnnouncement = false)
            }
        }
    }

    override fun close() {
        timeoutJob?.cancel()
        startRequestId?.let(tts::stopIfActive)
        successOwner = null
        scope.cancel()
    }

    private fun speak(@StringRes message: Int, owner: String, phase: String): Long? =
        runCatching {
            val language = localized.resources.configuration.locales[0].toLanguageTag()
            tts.interruptAndSpeak(
                TtsRequest(
                    text = localized.getString(message),
                    consumer = TtsConsumer.AUTO_SPEAK,
                    priority = TtsPriority.USER,
                    requestMode = TtsRequestMode.INTERRUPT,
                    settingsSnapshot = app.appContainer.repository.currentGlobalTtsSettings()
                        .toRuntimeSnapshot(targetLanguage = language),
                    ownerToken = owner,
                ),
            )
        }.onSuccess {
            PhoneControlLog.i(TAG, "setup_voice_result phase=$phase result=requested")
        }.onFailure {
            PhoneControlLog.e(TAG, "setup_voice_result phase=$phase result=request_failed", it)
        }.getOrNull()

    private fun completeSuccessSpeech(result: String) {
        if (successOwner == null) return
        successOwner = null
        timeoutJob?.cancel()
        timeoutJob = null
        startRequestId = null
        PhoneControlLog.i(TAG, "setup_voice_result phase=success result=$result")
        runtime()?.authoritySetupAnnouncementFinished()
    }

    private fun abandonPendingSuccessSpeech() {
        successGeneration += 1
        successOwner = null
        timeoutJob?.cancel()
        timeoutJob = null
    }

    private companion object {
        const val TAG = "SGTPhoneControlService"
        const val START_OWNER = "phone-control-setup-start"
        const val SUCCESS_OWNER_PREFIX = "phone-control-setup-success-"
        const val SUCCESS_SPEECH_TIMEOUT_MS = 20_000L
    }
}

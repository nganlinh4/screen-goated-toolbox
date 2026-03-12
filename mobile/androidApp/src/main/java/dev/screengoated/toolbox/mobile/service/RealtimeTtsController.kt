package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.AudioAttributes
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.util.Log
import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import java.util.concurrent.ConcurrentHashMap
import java.util.Locale
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

class RealtimeTtsController(
    context: Context,
) : TextToSpeech.OnInitListener {
    private val appContext = context.applicationContext
    private val audioManager = appContext.getSystemService(AudioManager::class.java)
    private val ready = AtomicBoolean(false)
    private var tts: TextToSpeech? = TextToSpeech(appContext, this)
    private val spokenLength = AtomicInteger(0)
    private val queuedLength = AtomicInteger(0)
    private val appliedSettings = AtomicReference<TtsRenderSettings?>(null)
    private val utteranceOffsets = ConcurrentHashMap<String, Int>()
    private val hasAudioFocus = AtomicBoolean(false)
    private val outputAudioAttributes = buildOutputAudioAttributes()
    private val audioFocusListener = AudioManager.OnAudioFocusChangeListener { focusChange ->
        if (focusChange == AudioManager.AUDIOFOCUS_LOSS ||
            focusChange == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT
        ) {
            stop()
        }
    }
    private val audioFocusRequest = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK)
        .setAudioAttributes(outputAudioAttributes)
        .setOnAudioFocusChangeListener(audioFocusListener)
        .build()

    override fun onInit(status: Int) {
        val initialized = status == TextToSpeech.SUCCESS
        ready.set(initialized)
        if (initialized) {
            configureOutput(tts)
            configureProgressListener(tts)
        }
    }

    fun stop() {
        queuedLength.set(spokenLength.get())
        utteranceOffsets.clear()
        tts?.stop()
        abandonAudioFocus()
    }

    fun stopAndReset() {
        spokenLength.set(0)
        queuedLength.set(0)
        appliedSettings.set(null)
        utteranceOffsets.clear()
        tts?.stop()
        abandonAudioFocus()
    }

    fun shutdown() {
        spokenLength.set(0)
        queuedLength.set(0)
        appliedSettings.set(null)
        utteranceOffsets.clear()
        tts?.stop()
        tts?.shutdown()
        tts = null
        abandonAudioFocus()
        ready.set(false)
    }

    fun speakCommittedText(
        committedText: String,
        targetLanguage: String,
        settings: RealtimeTtsSettings,
    ) {
        if (!settings.enabled || !ready.get()) {
            if (!settings.enabled) {
                stopAndReset()
            }
            return
        }
        if (committedText.isBlank()) {
            spokenLength.set(0)
            queuedLength.set(0)
            utteranceOffsets.clear()
            appliedSettings.set(null)
            return
        }

        val normalized = committedText.trimEnd()
        if (spokenLength.get() == 0 && queuedLength.get() == 0 && normalized.length > 50) {
            val boundary = normalized.dropLast(1).lastIndexOfAny(charArrayOf('.', '?', '!', '\n'))
            if (boundary > 0) {
                val skipTo = boundary + 1
                spokenLength.set(skipTo)
                queuedLength.set(skipTo)
            }
        }
        val renderSettings = TtsRenderSettings(
            targetLanguage = targetLanguage,
            speedPercent = settings.speedPercent,
            autoSpeed = settings.autoSpeed,
            volumePercent = settings.volumePercent,
        )
        if (appliedSettings.get() != renderSettings) {
            stop()
            appliedSettings.set(renderSettings)
        }

        val spokenChars = spokenLength.get()
        val queuedChars = queuedLength.get()
        if (normalized.length <= queuedChars) {
            return
        }

        val nextSegment = normalized.substring(queuedChars).trim()
        if (nextSegment.isBlank()) {
            queuedLength.set(normalized.length)
            return
        }

        val engine = tts ?: return
        requestAudioFocus()
        engine.language = localeForLanguage(targetLanguage)
        engine.setSpeechRate(speechRate(settings, normalized.length - spokenChars))
        val params = Bundle().apply {
            putFloat(TextToSpeech.Engine.KEY_PARAM_VOLUME, settings.volumePercent.coerceIn(0, 100) / 100f)
        }
        val utteranceId = "sgt-realtime-${SystemClock.elapsedRealtime()}"
        utteranceOffsets[utteranceId] = normalized.length
        engine.speak(
            nextSegment,
            TextToSpeech.QUEUE_ADD,
            params,
            utteranceId,
        )
        queuedLength.set(normalized.length)
    }

    private fun localeForLanguage(targetLanguage: String): Locale {
        val code = LanguageCatalog.codeForName(targetLanguage).lowercase(Locale.US)
        return Locale.forLanguageTag(code).takeIf { it.language.isNotBlank() } ?: Locale.getDefault()
    }

    private fun speechRate(settings: RealtimeTtsSettings, pendingChars: Int): Float {
        val baseRate = settings.speedPercent.coerceIn(50, 200) / 100f
        if (!settings.autoSpeed) {
            return baseRate
        }
        val catchUpBoost = when {
            pendingChars > 320 -> 1.45f
            pendingChars > 180 -> 1.25f
            pendingChars > 90 -> 1.1f
            else -> 1f
        }
        return (baseRate * catchUpBoost).coerceIn(0.5f, 2f)
    }

    private fun configureOutput(engine: TextToSpeech?) {
        val target = engine ?: return
        runCatching {
            target.setAudioAttributes(outputAudioAttributes)
        }
    }

    private fun configureProgressListener(engine: TextToSpeech?) {
        engine?.setOnUtteranceProgressListener(
            object : UtteranceProgressListener() {
                override fun onStart(utteranceId: String) = Unit

                override fun onDone(utteranceId: String) {
                    val offset = utteranceOffsets.remove(utteranceId) ?: return
                    spokenLength.getAndUpdate { current -> maxOf(current, offset) }
                    abandonAudioFocusIfIdle()
                }

                override fun onError(utteranceId: String) {
                    utteranceOffsets.remove(utteranceId)
                    queuedLength.set(spokenLength.get())
                    abandonAudioFocusIfIdle()
                }

                override fun onError(
                    utteranceId: String,
                    errorCode: Int,
                ) {
                    onError(utteranceId)
                }

                override fun onStop(
                    utteranceId: String,
                    interrupted: Boolean,
                ) {
                    utteranceOffsets.remove(utteranceId)
                    abandonAudioFocusIfIdle()
                }
            },
        )
    }

    private fun requestAudioFocus() {
        if (hasAudioFocus.get()) {
            return
        }
        val manager = audioManager ?: return
        val result = manager.requestAudioFocus(audioFocusRequest)
        if (result == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
            hasAudioFocus.set(true)
        } else {
            Log.d(TAG, "Audio focus request for TTS was not granted: result=$result")
        }
    }

    private fun abandonAudioFocusIfIdle() {
        if (utteranceOffsets.isEmpty() && queuedLength.get() <= spokenLength.get()) {
            abandonAudioFocus()
        }
    }

    private fun abandonAudioFocus() {
        if (!hasAudioFocus.getAndSet(false)) {
            return
        }
        audioManager?.abandonAudioFocusRequest(audioFocusRequest)
    }

    private fun buildOutputAudioAttributes(): AudioAttributes {
        return AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_ASSISTANT)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
            .apply {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    setAllowedCapturePolicy(AudioAttributes.ALLOW_CAPTURE_BY_NONE)
                }
            }
            .build()
    }

    private data class TtsRenderSettings(
        val targetLanguage: String,
        val speedPercent: Int,
        val autoSpeed: Boolean,
        val volumePercent: Int,
    )

    private companion object {
        private const val TAG = "SGTTts"
    }
}

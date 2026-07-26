package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.preset.TaalasClient
import java.util.Locale
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient

internal fun interface PhoneControlEmotionClassifier {
    fun classify(reply: String): String?
}

internal class TaalasPhoneControlEmotionClassifier(
    private val httpClient: OkHttpClient,
) : PhoneControlEmotionClassifier {
    override fun classify(reply: String): String? =
        TaalasClient.generate(httpClient, emotionPrompt(reply))
}

internal class PhoneControlOrbEmotionController(
    scope: CoroutineScope,
    private val classifier: PhoneControlEmotionClassifier,
    private val publishIcon: (String) -> Unit,
    private val cadenceMs: Long = CLASSIFICATION_CADENCE_MS,
) {
    private val lock = Any()
    private var respondingEpoch = 0L
    private var responding = false
    private var latestReply = ""
    private var lastClassifiedReply = ""
    private var lastIcon: String? = null

    init {
        require(cadenceMs > 0)
        scope.launch {
            while (isActive) {
                delay(cadenceMs)
                classifyLatest()
            }
        }
    }

    fun observePresentation(stateLabel: String) {
        synchronized(lock) {
            val nextResponding =
                stateLabel == GeneratedPhoneControlContract.ORB_STATE_RESPONDING
            if (nextResponding == responding) return
            responding = nextResponding
            respondingEpoch = nextOrdinal(respondingEpoch)
            latestReply = ""
            lastClassifiedReply = ""
            lastIcon = null
        }
    }

    fun observeReply(reply: String) {
        val bounded = reply.take(MAXIMUM_INPUT_CHARACTERS)
        synchronized(lock) {
            if (responding) latestReply = bounded
        }
    }

    private fun classifyLatest() {
        val candidate = synchronized(lock) {
            latestReply
                .takeIf { responding && it.isNotBlank() && it != lastClassifiedReply }
                ?.let { respondingEpoch to it }
        } ?: return
        val (epoch, reply) = candidate
        Log.d(TAG, "emotion_classification_requested epoch=$epoch chars=${reply.length}")
        val icon = runCatching { emotionIcon(classifier.classify(reply)) }
            .onFailure {
                Log.w(
                    TAG,
                    "emotion_classification_failed epoch=$epoch type=${it.javaClass.simpleName}",
                )
            }
            .getOrNull()
        val shouldPublish = synchronized(lock) {
            if (!responding || respondingEpoch != epoch || latestReply != reply) {
                false
            } else {
                lastClassifiedReply = reply
                val changed = icon != null && icon != lastIcon
                if (changed) lastIcon = icon
                changed
            }
        }
        if (shouldPublish) {
            publishIcon(requireNotNull(icon))
            Log.d(TAG, "emotion_classification_applied epoch=$epoch icon=$icon")
        }
    }
}

internal fun emotionIcon(response: String?): String? {
    val normalized = response
        ?.lowercase(Locale.ROOT)
        ?.replace(' ', '_')
        ?: return null
    return EMOTION_ICONS.firstOrNull { (label, _) -> label in normalized }?.second
        ?: SENTIMENT_NEUTRAL
}

private fun emotionPrompt(reply: String): String =
    "You label the emotional TONE of an assistant's spoken reply. Respond with EXACTLY ONE " +
        "of these labels and nothing else: ${EMOTION_PROMPT_LABELS.joinToString()}.\n\n" +
        "Reply: $reply"

private fun nextOrdinal(value: Long): Long = if (value == Long.MAX_VALUE) 1L else value + 1L

private const val TAG = "SGTPhoneControlEmotion"
private const val CLASSIFICATION_CADENCE_MS = 1_000L
private const val MAXIMUM_INPUT_CHARACTERS = 600
private const val SENTIMENT_NEUTRAL = "sentiment_neutral"
private val EMOTION_PROMPT_LABELS = listOf(
    "very_satisfied",
    "satisfied",
    "excited",
    "content",
    "calm",
    "neutral",
    "worried",
    "stressed",
    "frustrated",
    "sad",
    "dissatisfied",
    "very_dissatisfied",
    "extremely_dissatisfied",
)
private val EMOTION_ICONS = listOf(
    "extremely_dissatisfied" to "sentiment_extremely_dissatisfied",
    "very_dissatisfied" to "sentiment_very_dissatisfied",
    "very_satisfied" to "sentiment_very_satisfied",
    "dissatisfied" to "sentiment_dissatisfied",
    "satisfied" to "sentiment_satisfied",
    "frustrated" to "sentiment_frustrated",
    "stressed" to "sentiment_stressed",
    "excited" to "sentiment_excited",
    "worried" to "sentiment_worried",
    "content" to "sentiment_content",
    "neutral" to SENTIMENT_NEUTRAL,
    "calm" to "sentiment_calm",
    "sad" to "sentiment_sad",
)

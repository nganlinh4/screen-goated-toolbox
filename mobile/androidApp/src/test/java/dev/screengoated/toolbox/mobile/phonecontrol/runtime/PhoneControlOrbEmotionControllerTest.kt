package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PhoneControlOrbEmotionControllerTest {
    @Test
    fun `labels are matched most specific first and malformed replies stay neutral`() {
        assertEquals(
            "sentiment_extremely_dissatisfied",
            emotionIcon("extremely dissatisfied"),
        )
        assertEquals("sentiment_very_satisfied", emotionIcon("VERY_SATISFIED"))
        assertEquals("sentiment_neutral", emotionIcon("unexpected prose"))
        assertNull(emotionIcon(null))
    }

    @Test
    fun `responding captions classify asynchronously without duplicate icon churn`() = runTest {
        val replies = mutableListOf<String>()
        val icons = mutableListOf<String>()
        val controller = PhoneControlOrbEmotionController(
            scope = backgroundScope,
            classifier = PhoneControlEmotionClassifier {
                replies += it
                "excited"
            },
            publishIcon = icons::add,
            cadenceMs = 100L,
        )

        controller.observePresentation(GeneratedPhoneControlContract.ORB_STATE_RESPONDING)
        controller.observeReply("The task is finished.")
        advanceTimeBy(100L)
        runCurrent()
        controller.observeReply("The task is finished.")
        advanceTimeBy(100L)
        runCurrent()

        assertEquals(listOf("The task is finished."), replies)
        assertEquals(listOf("sentiment_excited"), icons)
    }

    @Test
    fun `reply text outside responding state never reaches the classifier`() = runTest {
        var classifications = 0
        val controller = PhoneControlOrbEmotionController(
            scope = backgroundScope,
            classifier = PhoneControlEmotionClassifier {
                classifications += 1
                "calm"
            },
            publishIcon = {},
            cadenceMs = 100L,
        )

        controller.observeReply("internal error text")
        advanceTimeBy(300L)
        runCurrent()

        assertEquals(0, classifications)
    }
}

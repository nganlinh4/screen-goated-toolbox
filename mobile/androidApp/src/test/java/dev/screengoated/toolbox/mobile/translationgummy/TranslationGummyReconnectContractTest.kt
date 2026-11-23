package dev.screengoated.toolbox.mobile.translationgummy

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEvent
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveHttpFailure
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionFailure
import dev.screengoated.toolbox.mobile.capture.startCaptureWithAdmission
import dev.screengoated.toolbox.mobile.shared.live.geminiLiveCloseRetryable
import java.io.File
import java.io.IOException
import java.net.ProtocolException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking

class TranslationGummyReconnectContractTest {
    private val fixture = run {
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) { it.parentFile }
            .first { File(it, "parity-fixtures/translation-gummy/reconnect-contract.json").isFile }
        Json.parseToJsonElement(File(root, "parity-fixtures/translation-gummy/reconnect-contract.json").readText()).jsonObject
    }

    @Test fun failureCasesFollowSharedContract() {
        fixture.getValue("failureCases").jsonArray.forEach { item ->
            val case = item.jsonObject
            val lifecycle = gummyLifecycle()
            var now = 1L
            lifecycle.reduce(now, GeminiLiveLifecycleEvent.Start)
            repeat(case["previousReconnects"]?.jsonPrimitive?.int ?: 0) {
                lifecycle.reduce(now, GeminiLiveLifecycleEvent.TransportFailure(lifecycle.state.generation, true))
                now = requireNotNull(lifecycle.state.reconnectDeadlineMs)
                lifecycle.reduce(now, GeminiLiveLifecycleEvent.Tick)
            }
            val retryable = when (case.getValue("kind").jsonPrimitive.content) {
                "close" -> geminiLiveCloseRetryable(case.getValue("code").jsonPrimitive.int)
                "server" -> gummyFailureRetryable(GeminiLiveSessionFailure.Server("failure", case.getValue("retryable").jsonPrimitive.boolean))
                else -> gummyFailureRetryable(GeminiLiveSessionFailure.Transport(
                    if (case.getValue("retryable").jsonPrimitive.boolean) IOException("temporary") else ProtocolException("invalid protocol"),
                ))
            }
            lifecycle.reduce(now, GeminiLiveLifecycleEvent.TransportFailure(lifecycle.state.generation, retryable))
            val expected = if (case.getValue("action").jsonPrimitive.content == "reconnect") GeminiLiveLifecyclePhase.BACKING_OFF else GeminiLiveLifecyclePhase.FAILED
            assertEquals(case.getValue("name").jsonPrimitive.content, expected, lifecycle.state.phase)
        }
    }

    @Test fun setupDoesNotResetRetryButMeaningfulContentDoes() {
        val lifecycle = gummyLifecycle()
        lifecycle.reduce(1, GeminiLiveLifecycleEvent.Start)
        lifecycle.reduce(2, GeminiLiveLifecycleEvent.TransportFailure(lifecycle.state.generation, true))
        val retryAt = requireNotNull(lifecycle.state.reconnectDeadlineMs)
        lifecycle.reduce(retryAt, GeminiLiveLifecycleEvent.Tick)
        val generation = lifecycle.state.generation
        lifecycle.reduce(retryAt, GeminiLiveLifecycleEvent.SocketOpened(generation))
        lifecycle.reduce(retryAt, GeminiLiveLifecycleEvent.Frame(GeminiLiveLifecycleFrame(generation, setupComplete = true)))
        assertEquals(1, lifecycle.state.reconnectAttempt)
        lifecycle.reduce(retryAt, GeminiLiveLifecycleEvent.Frame(GeminiLiveLifecycleFrame(generation, contentCount = 1)))
        assertEquals(0, lifecycle.state.reconnectAttempt)
    }

    @Test fun retiredOwnersCannotPublishAfterStopOrReplacement() {
        fixture.getValue("ownershipCases").jsonArray.forEach {
            val owner = TranslationGummyOwner()
            val old = owner.replace()
            owner.replace()
            var effects = 0
            owner.ifCurrent(old) { effects++ }
            assertEquals(it.jsonObject.getValue("name").jsonPrimitive.content, 0, effects)
        }
    }

    @Test fun retiredCaptureOpenIsReleasedWithoutStarting() {
        val owner = TranslationGummyOwner()
        val retired = owner.replace()
        owner.replace()
        var starts = 0
        var releases = 0
        val started = startCaptureWithAdmission(
            admit = { start -> owner.ifCurrent(retired) { start(); true } ?: false },
            start = { starts++ },
            release = { releases++ },
        )
        assertEquals(false, started)
        assertEquals(0, starts)
        assertEquals(1, releases)
    }

    @Test fun captureThatFinishesStartingAfterRetirementIsReleased() {
        val owner = TranslationGummyOwner()
        val retired = owner.replace()
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val cleaned = CountDownLatch(1)
        val worker = Thread {
            startCaptureWithAdmission(
                admit = { start -> if (!owner.isCurrent(retired)) false else { start(); owner.isCurrent(retired) } },
                start = { entered.countDown(); release.await() },
                release = { cleaned.countDown() },
            )
        }
        worker.start()
        try {
            check(entered.await(2, TimeUnit.SECONDS))
            owner.replace()
        } finally {
            release.countDown()
            worker.join(2_000)
        }
        assertEquals(0L, cleaned.count)
        assertEquals(false, worker.isAlive)
    }

    @Test fun handshakeHttpStatusRetainsRetryClassification() {
        for (status in listOf(408, 429, 500, 503)) {
            assertEquals(true, gummyTransportRetryable(GeminiLiveHttpFailure(status, ProtocolException())))
        }
        for (status in listOf(101, 401, 403)) {
            assertEquals(false, gummyTransportRetryable(GeminiLiveHttpFailure(status, ProtocolException())))
        }
        assertEquals(false, gummyTransportRetryable(ProtocolException("invalid upgrade")))
    }

    @Test fun stalledClassifierDoesNotPreventCancellation() = runBlocking {
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val job = launch(Dispatchers.Default) {
            TranslationGummyClassifier.detect("sample", {
                entered.countDown()
                while (release.count > 0) {
                    try { release.await() } catch (_: InterruptedException) { }
                }
                "eng"
            }, { "eng" })
        }
        try {
            check(entered.await(2, TimeUnit.SECONDS))
            job.cancelAndJoin()
            assertEquals(true, job.isCancelled)
        } finally {
            release.countDown()
        }
    }
}

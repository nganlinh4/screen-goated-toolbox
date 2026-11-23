package dev.screengoated.toolbox.mobile.service

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import dev.screengoated.toolbox.mobile.shared.live.GenerationPlaybackGate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Paths
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread

class S2sVisibilityGateTest {
    @Test
    fun directSpeechMatchesSharedGenerationFixture() {
        val paths = listOf(
            Paths.get("..", "parity-fixtures", "live-translate", "direct-speech-generation.json"),
            Paths.get("..", "..", "parity-fixtures", "live-translate", "direct-speech-generation.json"),
            Paths.get("parity-fixtures", "live-translate", "direct-speech-generation.json"),
        )
        val fixture = Json.parseToJsonElement(Files.readAllBytes(paths.first { Files.exists(it) }).decodeToString()).jsonObject
        fixture.getValue("cases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val gate = S2sVisibilityGate()
            var session = gate.start()
            val produced = mutableMapOf<String, Long?>()
            case.getValue("events").jsonArray.forEach { step ->
                val event = step.jsonObject
                when {
                    "produce" in event -> produced[event.getValue("produce").jsonPrimitive.content] = gate.snapshot()
                    "hide" in event -> gate.setVisible(false)
                    "show" in event -> gate.setVisible(true)
                    "stop" in event -> assertTrue(gate.stop(session))
                    "restart" in event -> { session = gate.start() }
                    "deliver" in event -> {
                        val epoch = produced.getValue(event.getValue("deliver").jsonPrimitive.content)
                        val delivered = gate.dispatch(epoch) {}
                        assertEquals(case.getValue("name").jsonPrimitive.content, event.getValue("expected").jsonPrimitive.boolean, delivered)
                    }
                }
            }
        }
    }

    @Test
    fun oldSessionCannotStopReplacement() {
        val gate = S2sVisibilityGate()
        val old = gate.start()
        val current = gate.start()
        assertFalse(gate.stop(old))
        assertTrue(gate.isCurrent(gate.snapshot()))
        assertTrue(gate.stop(current))
    }

    @Test
    fun delayedCapturedInputCannotReauthorizeOldProviderStream() {
        val gate = S2sVisibilityGate()
        gate.start()
        val original = gate.snapshot()
        val stream = S2sStreamEpoch(original)
        assertEquals(S2sStreamInput.KEEP, stream.observe(original, gate))
        gate.setVisible(false)
        assertEquals(S2sStreamInput.DROP, stream.observe(original, gate))
        assertEquals(S2sStreamInput.KEEP, stream.observe(null, gate))
        gate.setVisible(true)
        assertEquals(S2sStreamInput.DROP, stream.observe(null, gate))
        assertEquals(S2sStreamInput.DROP, stream.observe(original, gate))
        val replacement = gate.snapshot()
        assertEquals(S2sStreamInput.RESTART, stream.observe(replacement, gate))
        assertEquals(replacement, stream.current())
        assertEquals(S2sStreamInput.KEEP, stream.observe(replacement, gate))
    }

    @Test
    fun hideWaitsForAdmittedEffectAndRejectsLaterEffects() {
        val gate = S2sVisibilityGate()
        gate.start()
        val epoch = gate.snapshot()
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val output = thread {
            gate.dispatch(epoch) {
                entered.countDown()
                check(release.await(5, TimeUnit.SECONDS))
            }
        }
        assertTrue(entered.await(5, TimeUnit.SECONDS))
        val hide = thread { gate.setVisible(false) }
        release.countDown()
        output.join(5_000)
        hide.join(5_000)
        assertFalse(output.isAlive)
        assertFalse(hide.isAlive)
        assertFalse(gate.dispatch(epoch) {})
    }

    @Test
    fun showWaitsUntilPriorHideHasStoppedItsPlayer() {
        val stopEntered = CountDownLatch(1)
        val releaseStop = CountDownLatch(1)
        val showStarted = CountDownLatch(1)
        val showFinished = CountDownLatch(1)
        val controller = S2sVisibilityController {
            stopEntered.countDown()
            check(releaseStop.await(5, TimeUnit.SECONDS))
        }
        controller.start()
        val old = controller.gate.snapshot()
        val hide = thread { controller.setVisible(false) }
        assertTrue(stopEntered.await(5, TimeUnit.SECONDS))
        val show = thread { showStarted.countDown(); controller.setVisible(true); showFinished.countDown() }
        assertTrue(showStarted.await(5, TimeUnit.SECONDS))
        assertFalse(showFinished.await(100, TimeUnit.MILLISECONDS))
        releaseStop.countDown()
        hide.join(5_000)
        show.join(5_000)
        assertFalse(hide.isAlive)
        assertFalse(show.isAlive)
        assertFalse(controller.gate.isCurrent(old))
        assertTrue(controller.gate.isCurrent(controller.gate.snapshot()))
    }

    @Test
    fun providerInterruptionCanRetireBetweenBoundedWrites() {
        val gate = GenerationPlaybackGate()
        val chunk = gate.tag(byteArrayOf(1))
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val staleDispatched = AtomicBoolean(false)
        val writer = thread {
            assertTrue(gate.dispatch(chunk) { entered.countDown() })
            check(release.await(5, TimeUnit.SECONDS))
            staleDispatched.set(gate.dispatch(chunk) {})
        }
        assertTrue(entered.await(5, TimeUnit.SECONDS))
        gate.invalidate()
        assertFalse(gate.isCurrent(chunk))
        release.countDown()
        writer.join(5_000)
        assertFalse(writer.isAlive)
        assertFalse(staleDispatched.get())
    }
}

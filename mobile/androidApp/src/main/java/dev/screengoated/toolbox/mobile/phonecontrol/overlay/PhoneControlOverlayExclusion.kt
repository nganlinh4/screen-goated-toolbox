package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

internal data class OverlayBounds(
    val left: Int,
    val top: Int,
    val right: Int,
    val bottom: Int,
) {
    init {
        require(right >= left && bottom >= top) { "overlay bounds must be normalized" }
    }

    fun contains(x: Int, y: Int): Boolean =
        x >= left && x < right && y >= top && y < bottom

    fun intersects(other: OverlayBounds): Boolean =
        left < other.right && other.left < right && top < other.bottom && other.top < bottom
}

internal interface PhoneControlOverlayExclusionParticipant {
    suspend fun <T> withOverlayHidden(block: suspend () -> T): T

    suspend fun <T> withOverlayAvoiding(bounds: OverlayBounds, block: suspend () -> T): T =
        withOverlayHidden(block)

    fun orbBounds(): OverlayBounds?
}

internal object PhoneControlOverlayExclusion {
    private val participant = AtomicReference<PhoneControlOverlayExclusionParticipant?>()
    private val transitionDepth = AtomicInteger(0)

    /** True only while SGT-owned overlay windows are being hidden or restored. */
    internal val controllerTransitionActive: Boolean
        get() = transitionDepth.get() > 0

    fun register(candidate: PhoneControlOverlayExclusionParticipant) {
        participant.set(candidate)
    }

    fun unregister(candidate: PhoneControlOverlayExclusionParticipant) {
        participant.compareAndSet(candidate, null)
    }

    suspend fun <T> forCapture(block: suspend () -> T): T =
        withControllerOverlayHidden(participant.get(), block)

    suspend fun <T> forPoint(
        x: Float,
        y: Float,
        block: suspend () -> T,
    ): T {
        val current = participant.get()
        val needsExclusion = current?.orbBounds()?.contains(x.toInt(), y.toInt()) == true
        val point = OverlayBounds(x.toInt(), y.toInt(), x.toInt() + 1, y.toInt() + 1)
        return if (needsExclusion) withControllerOverlayAvoiding(current, point, block) else block()
    }

    suspend fun <T> forSegment(
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
        block: suspend () -> T,
    ): T {
        val current = participant.get()
        val bounds = current?.orbBounds()
        val pathBounds = OverlayBounds(
            minOf(fromX, toX).toInt(),
            minOf(fromY, toY).toInt(),
            maxOf(fromX, toX).toInt() + 1,
            maxOf(fromY, toY).toInt() + 1,
        )
        return if (bounds?.intersects(pathBounds) == true) {
            withControllerOverlayAvoiding(current, pathBounds, block)
        } else {
            block()
        }
    }

    private suspend fun <T> withControllerOverlayAvoiding(
        current: PhoneControlOverlayExclusionParticipant,
        bounds: OverlayBounds,
        block: suspend () -> T,
    ): T {
        transitionDepth.incrementAndGet()
        return try {
            current.withOverlayAvoiding(bounds, block)
        } finally {
            val remaining = transitionDepth.decrementAndGet()
            check(remaining >= 0) { "Controller overlay transition depth underflow" }
        }
    }

    private suspend fun <T> withControllerOverlayHidden(
        current: PhoneControlOverlayExclusionParticipant?,
        block: suspend () -> T,
    ): T {
        if (current == null) return block()
        transitionDepth.incrementAndGet()
        return try {
            current.withOverlayHidden(block)
        } finally {
            val remaining = transitionDepth.decrementAndGet()
            check(remaining >= 0) { "Controller overlay transition depth underflow" }
        }
    }
}

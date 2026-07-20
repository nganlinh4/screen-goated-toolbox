package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlGenerationId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PHONE_CONTROL_TURN_POLICY
import java.util.ArrayDeque

internal data class PhoneControlHeldToolRejection(
    val id: String,
    val name: String,
    val generation: PhoneControlGenerationId?,
    val code: String,
)

/** Bounded response metadata only. This never queues executable tool work. */
internal class PhoneControlHeldToolRejections(
    private val capacity: Int = PHONE_CONTROL_TURN_POLICY.maximumHeldToolRejections,
) {
    private val records = ArrayDeque<PhoneControlHeldToolRejection>(capacity)

    var overflowGeneration: PhoneControlGenerationId? = null
        private set

    val size: Int
        get() = records.size

    var overflowed: Boolean = false
        private set

    init {
        require(capacity > 0) { "held rejection capacity must be positive" }
    }

    fun hold(rejection: PhoneControlHeldToolRejection) {
        if (overflowed) return
        if (records.size >= capacity) {
            overflowed = true
            overflowGeneration = rejection.generation
            return
        }
        records.addLast(rejection)
    }

    fun latchOverflow(generation: PhoneControlGenerationId?) {
        if (overflowed) return
        overflowed = true
        overflowGeneration = generation
    }

    fun drainFor(generation: PhoneControlGenerationId?): List<PhoneControlHeldToolRejection> {
        if (overflowed) return emptyList()
        return buildList(records.size) {
            while (records.isNotEmpty()) {
                records.removeFirst().takeIf { it.generation == generation }?.let(::add)
            }
        }
    }

    fun discardHeld() {
        records.clear()
    }

    fun abandonOverflow(): Boolean {
        if (!overflowed) return false
        records.clear()
        overflowed = false
        overflowGeneration = null
        return true
    }

    fun reset() {
        records.clear()
        overflowed = false
        overflowGeneration = null
    }
}

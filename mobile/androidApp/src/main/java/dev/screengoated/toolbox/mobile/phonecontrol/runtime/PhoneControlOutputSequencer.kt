package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlGenerationId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlOutputChunk

internal class PhoneControlOutputSequencer {
    private var nextOutputSequence = 0L

    fun chunk(generation: PhoneControlGenerationId): PhoneControlOutputChunk {
        nextOutputSequence = if (nextOutputSequence == Long.MAX_VALUE) 0L else nextOutputSequence + 1L
        return PhoneControlOutputChunk(generation, nextOutputSequence)
    }

    fun nextOrdinal(current: Long): Long = if (current == Long.MAX_VALUE) 1L else current + 1L
}

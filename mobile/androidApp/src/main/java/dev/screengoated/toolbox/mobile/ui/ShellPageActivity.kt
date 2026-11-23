package dev.screengoated.toolbox.mobile.ui

import androidx.compose.runtime.compositionLocalOf
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase

/**
 * False for shell pager pages that are composed (warm) but not the settled page, so their
 * decorative loops stop scheduling morphs instead of recomposing off-screen tiles forever.
 */
internal val LocalShellPageActive = compositionLocalOf { true }

internal fun shellPageActive(page: Int, settledPage: Int): Boolean = page == settledPage

private val runningPhases = setOf(SessionPhase.STARTING, SessionPhase.LISTENING, SessionPhase.TRANSLATING)

/** The only Live Translate fact the shell pages render; avoids passing the whole session state. */
internal fun SessionPhase.isLiveSessionRunning(): Boolean = this in runningPhases

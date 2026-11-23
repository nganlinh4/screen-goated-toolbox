package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import kotlinx.coroutines.Job

/**
 * Owns at most one refine per result window. A new refine, a close or a
 * dismiss-all cancels the running one, and callbacks that the cancelled
 * refine already queued are ignored because their token is no longer current.
 */
internal class PresetRefineJobs {
    private val tokens = HashMap<PresetResultWindowId, Long>()
    private val jobs = HashMap<PresetResultWindowId, Job>()
    private var nextToken = 0L

    /** Cancels any refine running in [id] and returns the token of the new one. */
    @Synchronized
    fun begin(id: PresetResultWindowId): Long {
        cancelLocked(id)
        nextToken += 1
        tokens[id] = nextToken
        return nextToken
    }

    /** Binds the launched job; a refine that was already superseded or finished is cancelled. */
    @Synchronized
    fun attach(id: PresetResultWindowId, token: Long, job: Job) {
        if (tokens[id] == token) jobs[id] = job else job.cancel()
    }

    @Synchronized
    fun isCurrent(id: PresetResultWindowId, token: Long): Boolean = tokens[id] == token

    /** Releases a completed refine. Returns false when it was superseded. */
    @Synchronized
    fun finish(id: PresetResultWindowId, token: Long): Boolean {
        if (tokens[id] != token) return false
        tokens.remove(id)
        jobs.remove(id)
        return true
    }

    @Synchronized
    fun cancel(id: PresetResultWindowId) = cancelLocked(id)

    @Synchronized
    fun cancelAll() {
        tokens.clear()
        jobs.values.forEach(Job::cancel)
        jobs.clear()
    }

    private fun cancelLocked(id: PresetResultWindowId) {
        tokens.remove(id)
        jobs.remove(id)?.cancel()
    }
}

internal const val STANDALONE_RESULT_PRESET_PREFIX = "standalone:"

internal fun isStandaloneResultPreset(presetId: String): Boolean =
    presetId.startsWith(STANDALONE_RESULT_PRESET_PREFIX)

/** A standalone window (help, markdown viewer) has no preset chain to refine with. */
internal fun refineSupported(presetId: String): Boolean = !isStandaloneResultPreset(presetId)

internal fun resultWindowDisabledActions(
    presetId: String,
    state: PresetResultWindowState,
): Set<String> = if (refineSupported(presetId)) {
    resultDisabledActions(state)
} else {
    resultDisabledActions(state) + "edit"
}

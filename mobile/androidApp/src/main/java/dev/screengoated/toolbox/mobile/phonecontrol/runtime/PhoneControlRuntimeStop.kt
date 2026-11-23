package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import kotlinx.coroutines.Job

/**
 * Cancels the session job and runs [release] only once that job has completed,
 * so teardown never mutates coordinator, recorder or audio state beside a
 * transport loop that is still unwinding. A job that never ran completes inside
 * [Job.cancel], so its release runs on the caller with nothing left to race.
 */
internal fun stopAfterSessionJob(job: Job?, release: () -> Unit) {
    if (job == null) {
        release()
        return
    }
    job.invokeOnCompletion { release() }
    job.cancel()
}

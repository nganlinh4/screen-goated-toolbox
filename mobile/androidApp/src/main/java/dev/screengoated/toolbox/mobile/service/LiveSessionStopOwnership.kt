package dev.screengoated.toolbox.mobile.service

import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin

internal class LiveSessionEpoch {
    private var value = 0L
    @Synchronized fun advance(): Long { value = Math.addExact(value, 1); return value }
    @Synchronized fun current(): Long = value
    @Synchronized fun isCurrent(expected: Long): Boolean = expected == value
}

internal suspend fun awaitLiveSessionStop(job: Job?, isCurrent: (Job?) -> Boolean, finish: () -> Unit): Boolean {
    job?.cancelAndJoin()
    if (!isCurrent(job)) return false
    finish()
    return true
}

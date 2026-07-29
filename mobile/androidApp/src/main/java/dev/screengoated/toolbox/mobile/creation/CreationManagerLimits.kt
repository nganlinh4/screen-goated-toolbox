package dev.screengoated.toolbox.mobile.creation

internal const val MAXIMUM_QUEUED_JOBS_PER_TOOL = 50
internal const val MAXIMUM_PENDING_JOBS_PER_TOOL =
    MAXIMUM_QUEUED_JOBS_PER_TOOL + CreationContract.MAXIMUM_PARALLEL_JOBS
internal const val CONTINUATION_LIFETIME_MS = 24L * 60 * 60 * 1_000
internal const val MAXIMUM_TERMINAL_JOBS = 192

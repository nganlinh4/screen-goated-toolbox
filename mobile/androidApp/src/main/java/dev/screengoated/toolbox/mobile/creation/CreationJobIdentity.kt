package dev.screengoated.toolbox.mobile.creation

import java.util.concurrent.atomic.AtomicLong

internal fun creationStageIsBusy(stage: String): Boolean = stage in setOf(
    "preparing",
    "uploading",
    "generating",
    "segmenting",
    "refining",
    "finalizing",
)

internal fun creationContinuationIsLive(createdAtMs: Long, nowMs: Long, lifetimeMs: Long): Boolean =
    createdAtMs in (nowMs - lifetimeMs)..nowMs

internal fun newCreationDispatchId(tool: CreationTool, nowMs: Long, sequence: Long): String =
    "dispatch_${tool.wireName}_${nowMs}_$sequence"

internal fun newCreationJobId(tool: CreationTool, nowMs: Long, sequence: Long): String =
    "${tool.wireName}_${nowMs}_$sequence"

internal fun nextJobId(tool: CreationTool): String =
    newCreationJobId(tool, System.currentTimeMillis(), creationJobSequence.getAndIncrement())

internal fun nextDispatchId(tool: CreationTool): String =
    newCreationDispatchId(tool, System.currentTimeMillis(), creationJobSequence.getAndIncrement())

internal fun creationSaturatingBytes(left: Long, right: Long): Long =
    if (right > 0L && left > Long.MAX_VALUE - right) Long.MAX_VALUE else left + right

internal fun CreationJobStatus.withCreationElapsed(startedAtMs: Long?, nowMs: Long): CreationJobStatus =
    copy(
        elapsedMs = if (startedAtMs != null && creationStageIsBusy(stage)) {
            when {
                startedAtMs > nowMs -> 0L
                startedAtMs <= nowMs - CreationContract.MAXIMUM_JOB_RUNTIME_MS ->
                    CreationContract.MAXIMUM_JOB_RUNTIME_MS
                else -> nowMs - startedAtMs
            }
        } else {
            elapsedMs?.coerceIn(0L, CreationContract.MAXIMUM_JOB_RUNTIME_MS)
        },
        progressRatio = progressRatio?.takeIf { it.isFinite() }?.coerceIn(0.0, 1.0),
        estimatedTotalMs = estimatedTotalMs?.coerceIn(
            1L,
            CreationContract.MAXIMUM_JOB_RUNTIME_MS,
        ),
        timingSampleCount = timingSampleCount?.coerceIn(0L, MAXIMUM_PUBLIC_TIMING_SAMPLES),
    )

private val creationJobSequence = AtomicLong()

package dev.screengoated.toolbox.mobile.creation.runtime

import android.content.Context

/** Stable host contract implemented by the separately delivered creation runtime. */
interface CreationRuntimeFactory {
    fun createAutomation(
        context: Context,
        tool: String,
        slot: Int,
    ): CreationRuntimeEngine

    fun createDepthRuntime(): CreationDepthRuntime
}

interface CreationRuntimeEngine {
    suspend fun prepare(events: CreationRuntimeEventSink)

    suspend fun runJob(
        requestJson: String,
        events: CreationRuntimeEventSink,
    )

    fun destroy()
}

fun interface CreationRuntimeEventSink {
    fun emit(eventJson: String)
}

interface CreationDepthRuntime {
    fun createPreview(
        sourcePath: String,
        modelPath: String,
        targetPath: String,
    ): Boolean

    fun close()
}

package dev.screengoated.toolbox.mobile.creation.runtime

import android.content.Context

internal const val CREATION_RUNTIME_INSTALL_FAILURE =
    "Creation tools could not be installed. Try again."

/** Stable host contract implemented by the separately delivered creation runtime. */
interface CreationRuntimeFactory {
    /**
     * Versioned capability handshake.
     *
     * The JSON object contains `contractVersion`, `runtimeVersion`, product `features`, and
     * product `tools`. Engine details remain runtime-owned.
     */
    fun runtimeManifest(): String

    fun createEngine(
        context: Context,
        tool: String,
        executionIndex: Int,
    ): CreationRuntimeEngine
}

interface CreationRuntimeEngine {
    /** Returns whether this engine instance can accept the complete stable request JSON. */
    fun supportsRequest(requestJson: String): Boolean

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

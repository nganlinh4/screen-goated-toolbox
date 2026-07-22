package dev.screengoated.toolbox.mobile.creation.runtime

import android.content.Context
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull

internal sealed interface CreationRuntimeStatus {
    data object Missing : CreationRuntimeStatus
    data class Downloading(val progress: Float) : CreationRuntimeStatus
    data class Ready(val sizeBytes: Long) : CreationRuntimeStatus
    data class Failed(val message: String) : CreationRuntimeStatus
}

internal class CreationRuntimeManager private constructor(context: Context) {
    private val provider = CreationRuntimeProvider(context.applicationContext)

    val status: StateFlow<CreationRuntimeStatus> = provider.status

    fun startInstall() = provider.startInstall()

    fun factory(): CreationRuntimeFactory? = provider.factory()

    suspend fun awaitFactory(): CreationRuntimeFactory? {
        provider.factory()?.let { return it }
        provider.startInstall()
        val terminal = withTimeoutOrNull(RUNTIME_WAIT_MS) {
            status.first {
                it is CreationRuntimeStatus.Ready || it is CreationRuntimeStatus.Failed
            }
        } ?: return null
        return if (terminal is CreationRuntimeStatus.Ready) provider.factory() else null
    }

    fun delete() = provider.delete()

    companion object {
        private const val RUNTIME_WAIT_MS = 5 * 60 * 1000L
        @Volatile private var instance: CreationRuntimeManager? = null

        fun get(context: Context): CreationRuntimeManager = instance ?: synchronized(this) {
            instance ?: CreationRuntimeManager(context).also { instance = it }
        }
    }
}

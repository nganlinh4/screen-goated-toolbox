package dev.screengoated.toolbox.mobile.creation.runtime

import android.content.Context
import com.google.android.play.core.splitcompat.SplitCompat
import com.google.android.play.core.splitinstall.SplitInstallManager
import com.google.android.play.core.splitinstall.SplitInstallManagerFactory
import com.google.android.play.core.splitinstall.SplitInstallRequest
import com.google.android.play.core.splitinstall.SplitInstallSessionState
import com.google.android.play.core.splitinstall.SplitInstallStateUpdatedListener
import com.google.android.play.core.splitinstall.model.SplitInstallSessionStatus
import java.io.File
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

internal class CreationRuntimeProvider(private val context: Context) {
    private val splitManager: SplitInstallManager =
        SplitInstallManagerFactory.create(context.applicationContext)
    private val mutableStatus = MutableStateFlow(computeStatus())
    private val listener = SplitInstallStateUpdatedListener(::onInstallState)
    private var sessionId: Int? = null
    @Volatile private var loadedFactory: CreationRuntimeFactory? = null

    val status: StateFlow<CreationRuntimeStatus> = mutableStatus.asStateFlow()

    init {
        splitManager.registerListener(listener)
    }

    fun startInstall() {
        if (factory() != null || mutableStatus.value is CreationRuntimeStatus.Downloading) return
        mutableStatus.value = CreationRuntimeStatus.Downloading(0f)
        val request = SplitInstallRequest.newBuilder().addModule(MODULE_NAME).build()
        splitManager.startInstall(request)
            .addOnSuccessListener { sessionId = it }
            .addOnFailureListener { error ->
                mutableStatus.value = CreationRuntimeStatus.Failed(
                    error.message ?: "Creation runtime delivery failed",
                )
            }
    }

    fun factory(): CreationRuntimeFactory? {
        loadedFactory?.let { return it }
        if (MODULE_NAME !in splitManager.installedModules) return null
        return loadFactory()?.also {
            loadedFactory = it
            mutableStatus.value = CreationRuntimeStatus.Ready(installedBytes())
        }
    }

    fun delete() {
        sessionId?.let(splitManager::cancelInstall)
        sessionId = null
        loadedFactory = null
        splitManager.deferredUninstall(listOf(MODULE_NAME))
        mutableStatus.value = CreationRuntimeStatus.Missing
    }

    private fun onInstallState(state: SplitInstallSessionState) {
        if (MODULE_NAME !in state.moduleNames()) return
        sessionId = state.sessionId()
        when (state.status()) {
            SplitInstallSessionStatus.PENDING,
            SplitInstallSessionStatus.DOWNLOADING,
            SplitInstallSessionStatus.INSTALLING -> {
                val total = state.totalBytesToDownload()
                val progress = if (total > 0L) state.bytesDownloaded().toFloat() / total else 0f
                mutableStatus.value = CreationRuntimeStatus.Downloading(progress)
            }
            SplitInstallSessionStatus.INSTALLED -> {
                sessionId = null
                SplitCompat.install(context)
                val factory = loadFactory()
                if (factory == null) {
                    mutableStatus.value = CreationRuntimeStatus.Failed(
                        "Creation runtime split could not be loaded",
                    )
                } else {
                    loadedFactory = factory
                    mutableStatus.value = CreationRuntimeStatus.Ready(installedBytes())
                }
            }
            SplitInstallSessionStatus.FAILED -> {
                sessionId = null
                mutableStatus.value = CreationRuntimeStatus.Failed(
                    "Creation runtime delivery failed (${state.errorCode()})",
                )
            }
            SplitInstallSessionStatus.CANCELED -> {
                sessionId = null
                mutableStatus.value = computeStatus()
            }
        }
    }

    private fun loadFactory(): CreationRuntimeFactory? = runCatching {
        check(SplitCompat.install(context)) { "SplitCompat activation failed" }
        val type = Class.forName(FACTORY_CLASS, true, context.classLoader)
        type.getDeclaredConstructor().newInstance() as CreationRuntimeFactory
    }.getOrNull()

    private fun computeStatus(): CreationRuntimeStatus =
        if (MODULE_NAME in splitManager.installedModules) {
            CreationRuntimeStatus.Ready(installedBytes())
        } else {
            CreationRuntimeStatus.Missing
        }

    private fun installedBytes(): Long = context.applicationInfo.splitSourceDirs.orEmpty()
        .filter { MODULE_NAME in it }
        .sumOf { File(it).length() }

    private companion object {
        const val MODULE_NAME = "feature_creation_runtime"
        const val FACTORY_CLASS =
            "dev.screengoated.toolbox.creation.runtime.AndroidCreationRuntimeFactory"
    }
}

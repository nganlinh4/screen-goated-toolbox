package dev.screengoated.toolbox.mobile.creation.runtime

import android.content.Context
import com.google.android.play.core.splitcompat.SplitCompat
import com.google.android.play.core.splitinstall.SplitInstallManager
import com.google.android.play.core.splitinstall.SplitInstallManagerFactory
import com.google.android.play.core.splitinstall.SplitInstallRequest
import com.google.android.play.core.splitinstall.SplitInstallSessionState
import com.google.android.play.core.splitinstall.SplitInstallStateUpdatedListener
import com.google.android.play.core.splitinstall.model.SplitInstallSessionStatus
import dev.screengoated.toolbox.mobile.service.nativelibs.DeferredRemovalState
import dev.screengoated.toolbox.mobile.service.nativelibs.deferredRemovalState
import java.io.File
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

internal class CreationRuntimeProvider(private val context: Context) {
    private val splitManager: SplitInstallManager =
        SplitInstallManagerFactory.create(context.applicationContext)
    private val removalPreferences = context.applicationContext.getSharedPreferences(
        REMOVAL_PREFERENCES,
        Context.MODE_PRIVATE,
    )
    private val mutableStatus = MutableStateFlow(computeStatus())
    private val listener = SplitInstallStateUpdatedListener(::onInstallState)
    private var sessionId: Int? = null
    private var installRequested = false
    private var uninstallInFlight = false
    @Volatile private var loadedFactory: CreationRuntimeFactory? = null

    val status: StateFlow<CreationRuntimeStatus> = mutableStatus.asStateFlow()

    init {
        splitManager.registerListener(listener)
        if (removalPending()) requestRemoval()
    }

    fun startInstall() {
        if (removalPending() || factory() != null ||
            mutableStatus.value is CreationRuntimeStatus.Downloading
        ) return
        mutableStatus.value = CreationRuntimeStatus.Downloading(0f)
        installRequested = true
        val request = SplitInstallRequest.newBuilder().addModule(MODULE_NAME).build()
        splitManager.startInstall(request)
            .addOnSuccessListener {
                sessionId = it
                if (removalPending()) splitManager.cancelInstall(it)
            }
            .addOnFailureListener {
                installRequested = false
                if (removalPending()) {
                    requestRemoval()
                } else {
                    mutableStatus.value = CreationRuntimeStatus.Failed(
                        CREATION_RUNTIME_INSTALL_FAILURE,
                    )
                }
            }
    }

    fun factory(): CreationRuntimeFactory? {
        loadedFactory?.let { return it }
        if (removalPending() || MODULE_NAME !in splitManager.installedModules) return null
        return loadFactory()?.also {
            loadedFactory = it
            mutableStatus.value = CreationRuntimeStatus.Ready(installedBytes())
        }
    }

    fun delete() {
        sessionId?.let(splitManager::cancelInstall)
        loadedFactory = null
        setRemovalPending(true)
        mutableStatus.value = pendingStatus()
        requestRemoval()
    }

    private fun requestRemoval() {
        if (installRequested) {
            sessionId?.let(splitManager::cancelInstall)
            mutableStatus.value = pendingStatus()
            return
        }
        if (MODULE_NAME !in splitManager.installedModules) {
            completeRemoval()
            return
        }
        if (uninstallInFlight) return
        uninstallInFlight = true
        splitManager.deferredUninstall(listOf(MODULE_NAME))
            .addOnSuccessListener {
                uninstallInFlight = false
                if (MODULE_NAME in splitManager.installedModules) {
                    mutableStatus.value = pendingStatus()
                } else {
                    completeRemoval()
                }
            }
            .addOnFailureListener { error ->
                uninstallInFlight = false
                mutableStatus.value = CreationRuntimeStatus.RemovalPending(
                    error.message ?: "Play feature removal failed. Try again.",
                    retryable = true,
                )
            }
    }

    private fun completeRemoval() {
        setRemovalPending(false)
        mutableStatus.value = CreationRuntimeStatus.Missing
    }

    private fun onInstallState(state: SplitInstallSessionState) {
        if (MODULE_NAME !in state.moduleNames()) return
        sessionId = state.sessionId()
        when (state.status()) {
            SplitInstallSessionStatus.PENDING,
            SplitInstallSessionStatus.DOWNLOADING,
            SplitInstallSessionStatus.INSTALLING -> {
                if (removalPending()) {
                    mutableStatus.value = pendingStatus()
                    splitManager.cancelInstall(state.sessionId())
                    return
                }
                val total = state.totalBytesToDownload()
                val progress = if (total > 0L) state.bytesDownloaded().toFloat() / total else 0f
                mutableStatus.value = CreationRuntimeStatus.Downloading(progress)
            }
            SplitInstallSessionStatus.INSTALLED -> {
                installRequested = false
                sessionId = null
                SplitCompat.install(context)
                if (removalPending()) {
                    mutableStatus.value = pendingStatus()
                    requestRemoval()
                    return
                }
                val factory = loadFactory()
                if (factory == null) {
                    mutableStatus.value = CreationRuntimeStatus.Failed(
                        CREATION_RUNTIME_INSTALL_FAILURE,
                    )
                } else {
                    loadedFactory = factory
                    mutableStatus.value = CreationRuntimeStatus.Ready(installedBytes())
                }
            }
            SplitInstallSessionStatus.FAILED -> {
                installRequested = false
                sessionId = null
                if (removalPending()) {
                    requestRemoval()
                } else {
                    mutableStatus.value = CreationRuntimeStatus.Failed(
                        CREATION_RUNTIME_INSTALL_FAILURE,
                    )
                }
            }
            SplitInstallSessionStatus.CANCELED -> {
                installRequested = false
                sessionId = null
                if (removalPending()) requestRemoval() else mutableStatus.value = computeStatus()
            }
        }
    }

    private fun loadFactory(): CreationRuntimeFactory? = runCatching {
        check(SplitCompat.install(context)) { "SplitCompat activation failed" }
        val factoryClass = requireNotNull(loadCreationRuntimeFactoryClass(context)) {
            "Creation runtime delivery metadata is unavailable"
        }
        val type = Class.forName(factoryClass, true, context.classLoader)
        type.getDeclaredConstructor().newInstance() as CreationRuntimeFactory
    }.getOrNull()

    private fun computeStatus(): CreationRuntimeStatus {
        val installed = MODULE_NAME in splitManager.installedModules
        if (!installed && removalPending()) setRemovalPending(false)
        return when (deferredRemovalState(installed, removalPending())) {
            DeferredRemovalState.REMOVAL_PENDING -> pendingStatus()
            DeferredRemovalState.INSTALLED -> CreationRuntimeStatus.Ready(installedBytes())
            DeferredRemovalState.MISSING -> CreationRuntimeStatus.Missing
        }
    }

    private fun pendingStatus() = CreationRuntimeStatus.RemovalPending(
        "Google Play scheduled this runtime for removal.",
    )

    private fun removalPending(): Boolean =
        removalPreferences.getBoolean(REMOVAL_KEY, false)

    private fun setRemovalPending(pending: Boolean) {
        removalPreferences.edit().run {
            if (pending) putBoolean(REMOVAL_KEY, true) else remove(REMOVAL_KEY)
        }.apply()
    }

    private fun installedBytes(): Long = context.applicationInfo.splitSourceDirs.orEmpty()
        .filter { MODULE_NAME in it }
        .sumOf { File(it).length() }

    private companion object {
        const val MODULE_NAME = "feature_creation_runtime"
        const val REMOVAL_PREFERENCES = "creation_runtime_lifecycle"
        const val REMOVAL_KEY = "pending_play_removal"
    }
}

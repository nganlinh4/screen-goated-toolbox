package dev.screengoated.toolbox.mobile

import android.app.Application
import android.content.Context
import dev.screengoated.toolbox.mobile.creation.worker.CreationWorkerProcess
import dev.screengoated.toolbox.mobile.componentupdate.ComponentUpdateCatalog
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.storage.SecureBootstrap

class SgtMobileApplication : Application() {
    override fun attachBaseContext(base: Context) {
        super.attachBaseContext(base)
        CreationWorkerProcess.configureWebViewDataDirectory()
        installDistributionRuntime(this)
    }

    private val bootstrap = SecureBootstrap { AppContainer.open(this) }
    internal val appContainerState get() = bootstrap.state
    val appContainerOrNull: AppContainer? get() = bootstrap.current
    val appContainer: AppContainer get() = checkNotNull(appContainerOrNull) {
        "Encrypted settings are unavailable"
    }

    @Synchronized
    fun retrySecureSettings(): Boolean {
        if (bootstrap.current != null) return true
        if (bootstrap.retry() == null) return false
        if (BuildConfig.FLAVOR == "full") {
            ComponentUpdateCatalog.refreshInBackground(this)
        }
        return true
    }

    override fun onCreate() {
        super.onCreate()
        if (BuildConfig.FLAVOR == "full") {
            ComponentUpdateCatalog.loadCached(this)
        }
        if (CreationWorkerProcess.isWorkerProcess() ||
            !isPrimaryApplicationProcess(packageName, Application.getProcessName())
        ) {
            return
        }
        PhoneControlLog.initialize(this)
        retrySecureSettings()
    }
}

internal fun isPrimaryApplicationProcess(
    packageName: String,
    processName: String,
): Boolean = processName == packageName

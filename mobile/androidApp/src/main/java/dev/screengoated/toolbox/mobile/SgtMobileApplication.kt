package dev.screengoated.toolbox.mobile

import android.app.Application
import android.content.Context
import dev.screengoated.toolbox.mobile.creation.worker.CreationWorkerProcess
import dev.screengoated.toolbox.mobile.componentupdate.ComponentUpdateCatalog
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.service.moonshine.MoonshineModelManager
import dev.screengoated.toolbox.mobile.service.nativelibs.NativeLibManager
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

class SgtMobileApplication : Application() {
    private val maintenanceScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun attachBaseContext(base: Context) {
        super.attachBaseContext(base)
        CreationWorkerProcess.configureWebViewDataDirectory()
        if (BuildConfig.FLAVOR == "full") {
            ComponentUpdateCatalog.loadCached(this)
        }
        installDistributionRuntime(this)
    }

    lateinit var appContainer: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        if (CreationWorkerProcess.isWorkerProcess() ||
            !isPrimaryApplicationProcess(packageName, Application.getProcessName())
        ) {
            return
        }
        PhoneControlLog.initialize(this)
        if (BuildConfig.FLAVOR == "full") {
            ComponentUpdateCatalog.refreshInBackground(this)
        }
        appContainer = AppContainer(this)
        // Resume only persisted removals; this performs no runtime download or install.
        maintenanceScope.launch {
            NativeLibManager.reconcilePendingRemovals(this@SgtMobileApplication)
            MoonshineModelManager.reconcilePendingRemovals(this@SgtMobileApplication)
        }
    }
}

internal fun isPrimaryApplicationProcess(
    packageName: String,
    processName: String,
): Boolean = processName == packageName

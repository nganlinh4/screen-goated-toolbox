package dev.screengoated.toolbox.mobile

import android.app.Application
import android.content.Context
import dev.screengoated.toolbox.mobile.creation.CreationJobManager
import dev.screengoated.toolbox.mobile.creation.worker.CreationWorkerProcess
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog

class SgtMobileApplication : Application() {
    override fun attachBaseContext(base: Context) {
        super.attachBaseContext(base)
        CreationWorkerProcess.configureWebViewDataDirectory()
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
        appContainer = AppContainer(this)
        CreationJobManager.get(this).startOneShotPreparation()
    }
}

internal fun isPrimaryApplicationProcess(
    packageName: String,
    processName: String,
): Boolean = processName == packageName

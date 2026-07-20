package dev.screengoated.toolbox.mobile

import android.app.Application
import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog

class SgtMobileApplication : Application() {
    override fun attachBaseContext(base: Context) {
        super.attachBaseContext(base)
        installDistributionRuntime(this)
    }

    lateinit var appContainer: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        PhoneControlLog.initialize(this)
        appContainer = AppContainer(this)
    }
}

package dev.screengoated.toolbox.mobile

import android.app.Application

class SgtMobileApplication : Application() {
    lateinit var appContainer: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        appContainer = AppContainer(this)
    }
}


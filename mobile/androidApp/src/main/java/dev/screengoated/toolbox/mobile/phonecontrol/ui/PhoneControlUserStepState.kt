package dev.screengoated.toolbox.mobile.phonecontrol.ui

import androidx.lifecycle.ViewModel
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PlatformUserStepSlot

internal class PhoneControlUserStepState : ViewModel() {
    val permission = PlatformUserStepSlot()
    val settings = PlatformUserStepSlot()
    val shizuku = PlatformUserStepSlot()
    val root = PlatformUserStepSlot()

    override fun onCleared() {
        permission.finish()
        settings.finish()
        shizuku.finish()
        root.finish()
    }
}

package dev.screengoated.toolbox.mobile.phonecontrol.runtime

internal fun interface PhoneControlTurnCleanup {
    fun retire(turnId: Long)
}

internal val NoOpPhoneControlTurnCleanup = PhoneControlTurnCleanup { }

package dev.screengoated.toolbox.mobile.service

/**
 * The single owner of every fixed notification id the app posts. Two services
 * sharing an id replace each other's foreground notification, so a new id must
 * be added here and stay unique (ForegroundNotificationIdsTest).
 */
internal object ForegroundNotificationIds {
    const val LIVE_TRANSLATE = 1001
    const val BUBBLE = 1002
    const val PRESET_AUDIO = 1003
    const val DJ_PLAYBACK = 1004
    const val PHONE_CONTROL_SESSION = 4081
    const val PHONE_CONTROL_SETUP = 4083
    const val PHONE_CONTROL_ORPHAN_REENTRY = 4084
    const val PHONE_CONTROL_PROJECTION_BUSY = 4085
    const val PHONE_CONTROL_STOP_RETRY = 4086
    const val TRANSLATION_GUMMY = 42067
}

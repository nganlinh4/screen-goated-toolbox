package dev.screengoated.toolbox.mobile.ui

/** Admission and retirement are serialized with each callback's synchronous effect. */
internal class CustomModelActionLifetime {
    private var active = true

    @Synchronized fun run(action: () -> Boolean): Boolean = active && action()

    @Synchronized fun retire() { active = false }
}

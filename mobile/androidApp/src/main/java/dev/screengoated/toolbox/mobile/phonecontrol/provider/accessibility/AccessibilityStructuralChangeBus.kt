package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import java.io.Closeable
import java.util.concurrent.CopyOnWriteArraySet

internal object AccessibilityStructuralChangeBus {
    private val observers = CopyOnWriteArraySet<() -> Unit>()

    fun observe(observer: () -> Unit): Closeable {
        observers += observer
        return Closeable { observers -= observer }
    }

    fun publish() {
        observers.forEach { observer -> runCatching(observer) }
    }
}

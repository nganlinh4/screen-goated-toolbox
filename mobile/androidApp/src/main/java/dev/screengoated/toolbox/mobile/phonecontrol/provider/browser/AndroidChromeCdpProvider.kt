package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.provider.PhoneControlArtifactStore
import java.io.Closeable
import java.util.concurrent.atomic.AtomicReference

internal class AndroidChromeCdpProvider(
    context: Context,
    artifacts: PhoneControlArtifactStore,
) : Closeable {
    private val controller = ChromeCdpController(context, artifacts)

    init {
        PhoneControlBrowserLifecycle.attach(this)
    }

    suspend fun status(): BrowserProviderOutcome = controller.status()

    fun hasBoundTarget(): Boolean = controller.hasBoundTarget()

    suspend fun reset(): BrowserProviderOutcome = controller.reset()

    suspend fun tabs(): BrowserProviderOutcome = controller.tabs()

    suspend fun targetBaseline(): BrowserTargetBaselineResult =
        controller.targetBaseline()

    suspend fun attachLaunchedTarget(
        url: String,
        baseline: ChromeTargetBaseline,
    ): BrowserProviderOutcome = controller.attachLaunchedTarget(url, baseline)

    suspend fun openTab(
        url: String,
        lifetime: String,
        turnId: Long,
    ): BrowserProviderOutcome = controller.openTab(url, lifetime, turnId)

    suspend fun switchTab(handle: Int): BrowserProviderOutcome =
        controller.switchTab(handle)

    suspend fun closeTab(handle: Int): BrowserProviderOutcome =
        controller.closeTab(handle)

    suspend fun readPage(includePreview: Boolean): BrowserProviderOutcome =
        controller.readPage(includePreview)

    suspend fun navigate(url: String): BrowserProviderOutcome =
        controller.navigate(url)

    suspend fun history(direction: String): BrowserProviderOutcome =
        controller.history(direction)

    suspend fun waitFor(selector: String, timeoutMs: Long): BrowserProviderOutcome =
        controller.waitFor(selector, timeoutMs)

    suspend fun eval(code: String): BrowserProviderOutcome = controller.eval(code)

    suspend fun upload(selector: String, path: String): BrowserProviderOutcome =
        controller.upload(selector, path)

    suspend fun network(filter: String?): BrowserProviderOutcome =
        controller.network(filter)

    suspend fun console(): BrowserProviderOutcome = controller.console()

    suspend fun retireTurn(turnId: Long): BrowserTurnCleanupReceipt =
        controller.retireTurn(turnId)

    override fun close() = controller.close()
}

internal object PhoneControlBrowserLifecycle {
    private val active = AtomicReference<AndroidChromeCdpProvider?>()

    fun attach(provider: AndroidChromeCdpProvider) {
        active.getAndSet(provider)?.takeIf { it !== provider }?.close()
    }

    suspend fun retireTurn(turnId: Long): BrowserTurnCleanupReceipt =
        active.get()?.retireTurn(turnId) ?: BrowserTurnCleanupReceipt(0, 0, 0)

    fun close() {
        active.getAndSet(null)?.close()
    }
}

package dev.screengoated.toolbox.mobile.service.preset

import android.app.UiAutomation
import android.content.Context
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.view.WindowManager
import android.view.View
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiObject2
import java.io.File
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import org.json.JSONTokener
import org.json.JSONObject

internal class PresetUiAcceptanceSupport : AutoCloseable {
    val instrumentation = InstrumentationRegistry.getInstrumentation()
    val context = instrumentation.targetContext
    val device = UiDevice.getInstance(instrumentation)
    val automation = instrumentation.getUiAutomation(UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES)
    val repository = (context.applicationContext as SgtMobileApplication).appContainer.presetRepository
    val preferences = MutableStateFlow(MobileUiPreferences(uiLanguage = "en"))
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val created = mutableListOf<String>()
    private val originalActions = repository.postProcessActions
    private val overlayMode = Regex("SYSTEM_ALERT_WINDOW: (\\w+)")
        .find(shell("appops get ${context.packageName} SYSTEM_ALERT_WINDOW"))?.groupValues?.get(1) ?: "default"
    val controller: PresetOverlayController

    init {
        shell("appops set ${context.packageName} SYSTEM_ALERT_WINDOW allow")
        controller = main { PresetOverlayController(
            context, scope, context.getSystemService(WindowManager::class.java), repository,
            preferences, { preferences.value }, { false }, {}, {}, {},
        ) }
    }

    fun shell(command: String): String = automation.executeShellCommand(command).use {
        ParcelFileDescriptor.AutoCloseInputStream(it).bufferedReader().readText().trim()
    }

    fun <T> main(action: () -> T): T {
        val result = AtomicReference<Result<T>>()
        instrumentation.runOnMainSync { result.set(runCatching(action)) }
        return result.get().getOrThrow()
    }

    fun preset(type: PresetType = PresetType.TEXT_INPUT): String = main {
        repository.createCustomPreset(type, "en").also { id ->
            created += id
            repository.updateBuiltInOverride(id) { it.copy(
                nameEn = "Parity input", isFavorite = true, continuousInput = false,
                blocks = listOf(ProcessingBlock("input", BlockType.INPUT_ADAPTER, "", showOverlay = true)),
            ) }
        }
    }

    fun await(label: String, condition: () -> Boolean) {
        val deadline = SystemClock.elapsedRealtime() + 15_000
        while (SystemClock.elapsedRealtime() < deadline) {
            if (condition()) return
            SystemClock.sleep(100)
        }
        device.takeScreenshot(File(context.cacheDir, "parity-ui-failure.png"))
        device.dumpWindowHierarchy(File(context.cacheDir, "parity-ui-failure.xml"))
        error("Timed out: $label")
    }

    fun textControl(text: String): UiObject2 {
        var control: UiObject2? = null
        await("control $text") {
            control = device.findObject(By.text(text)) ?: device.findObject(By.desc(text))
            control != null
        }
        return requireNotNull(control)
    }

    fun js(window: PresetOverlayWindow, expression: String): Any? {
        val latch = CountDownLatch(1)
        val value = AtomicReference<String?>()
        main { window.runScriptForResult(expression) { value.set(it); latch.countDown() } }
        check(latch.await(5, TimeUnit.SECONDS)) { "WebView evaluation did not complete" }
        return value.get()?.let { JSONTokener(it).nextValue() }
    }

    fun tap(window: PresetOverlayWindow, selector: String) {
        val quoted = JSONObject.quote(selector)
        await("visible $selector") { js(window, "!!document.querySelector($quoted)") == true }
        val position = js(window, """(() => {
            const r=document.querySelector($quoted).getBoundingClientRect();
            return {x:(r.x+r.width/2)*devicePixelRatio,y:(r.y+r.height/2)*devicePixelRatio,w:r.width,h:r.height};
        })()""") as JSONObject
        check(position.getDouble("w") > 0 && position.getDouble("h") > 0)
        val origin = main {
            val field = PresetOverlayWindow::class.java.getDeclaredField("rootView")
            field.isAccessible = true
            IntArray(2).also { (field.get(window) as View).getLocationOnScreen(it) }
        }
        check(device.click(origin[0] + position.getDouble("x").toInt(), origin[1] + position.getDouble("y").toInt()))
    }

    override fun close() {
        main {
            controller.destroy()
            repository.postProcessActions = originalActions
            created.forEach(repository::deletePreset)
            scope.cancel()
        }
        shell("appops set ${context.packageName} SYSTEM_ALERT_WINDOW $overlayMode")
    }
}

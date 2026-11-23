package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import androidx.activity.result.ActivityResultRegistry
import androidx.activity.result.contract.ActivityResultContract
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.app.ActivityOptionsCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PhoneControlProjectionRegistryRestoreTest {
    @Test
    fun restoredResultDoesNotReachNewProcessLauncher() = runOnMain {
        val firstState = PhoneControlUserStepState()
        val oldRegistry = TestRegistry()
        val oldOwner = TestOwner()
        val oldLauncher = oldRegistry.register(
            firstState.projectionLauncherKey,
            oldOwner,
            ActivityResultContracts.StartActivityForResult(),
        ) { error("old activity is retired") }
        oldOwner.start()
        oldLauncher.launch(Intent())
        val oldRequestCode = oldRegistry.lastRequestCode
        val saved = Bundle()
        oldRegistry.onSaveInstanceState(saved)
        oldOwner.destroy()

        val newState = PhoneControlUserStepState()
        assertNotEquals(firstState.projectionLauncherKey, newState.projectionLauncherKey)
        val newRegistry = TestRegistry()
        newRegistry.onRestoreInstanceState(saved)
        val newOwner = TestOwner()
        var received = 0
        val newLauncher = newRegistry.register(
            newState.projectionLauncherKey,
            newOwner,
            ActivityResultContracts.StartActivityForResult(),
        ) { received += 1 }
        newOwner.start()
        newLauncher.launch(Intent())
        val newRequestCode = newRegistry.lastRequestCode

        newRegistry.dispatchResult(oldRequestCode, Activity.RESULT_OK, Intent())
        assertEquals(0, received)
        newRegistry.dispatchResult(newRequestCode, Activity.RESULT_OK, Intent())
        assertEquals(1, received)
        newOwner.destroy()
    }

    @Test
    fun configurationRecreationRetainsTheOriginalLauncherKey() = runOnMain {
        val state = PhoneControlUserStepState()
        val first = TestRegistry()
        val firstOwner = TestOwner()
        val launcher = first.register(
            state.projectionLauncherKey,
            firstOwner,
            ActivityResultContracts.StartActivityForResult(),
        ) { error("old activity is retired") }
        firstOwner.start()
        launcher.launch(Intent())
        val requestCode = first.lastRequestCode
        val saved = Bundle()
        first.onSaveInstanceState(saved)
        firstOwner.destroy()

        val restored = TestRegistry()
        restored.onRestoreInstanceState(saved)
        val newOwner = TestOwner()
        var received = 0
        restored.register(
            state.projectionLauncherKey,
            newOwner,
            ActivityResultContracts.StartActivityForResult(),
        ) { received += 1 }
        newOwner.start()
        restored.dispatchResult(requestCode, Activity.RESULT_OK, Intent())
        assertEquals(1, received)
        newOwner.destroy()
    }

    private fun runOnMain(action: () -> Unit) {
        InstrumentationRegistry.getInstrumentation().runOnMainSync(action)
    }

    private class TestOwner : LifecycleOwner {
        private val registry = LifecycleRegistry(this)
        override val lifecycle: Lifecycle get() = registry

        init { registry.handleLifecycleEvent(Lifecycle.Event.ON_CREATE) }
        fun start() { registry.handleLifecycleEvent(Lifecycle.Event.ON_START) }
        fun destroy() { registry.handleLifecycleEvent(Lifecycle.Event.ON_DESTROY) }
    }

    private class TestRegistry : ActivityResultRegistry() {
        var lastRequestCode = 0

        override fun <I, O> onLaunch(
            requestCode: Int,
            contract: ActivityResultContract<I, O>,
            input: I,
            options: ActivityOptionsCompat?,
        ) {
            lastRequestCode = requestCode
        }
    }
}

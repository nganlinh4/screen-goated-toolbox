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
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlProjectionDispatches
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlStopAdmission
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PhoneControlProjectionRegistryTest {
    @Test
    fun stopSupersedesStagedReplayAndRejectsQueuedDispatch() {
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            val dispatch = PhoneControlProjectionDispatches.create()
            try {
                assertTrue(PhoneControlProjectionDispatches.stageReentry(dispatch, Intent("new-action")))
                assertEquals(
                    PhoneControlStopAdmission.Accepted,
                    PhoneControlProjectionDispatches.beginStop(PhoneControlProjectionDispatches.stopBoundary()),
                )
                PhoneControlProjectionDispatches.endStop()
                assertNull(PhoneControlProjectionDispatches.admit(dispatch.id))
                assertNull(PhoneControlProjectionDispatches.claimReplay(dispatch))
                assertNull(PhoneControlProjectionDispatches.current())
            } finally {
                PhoneControlProjectionDispatches.endStop()
                PhoneControlProjectionDispatches.finish(dispatch)
                PhoneControlProjectionDispatches.claimReplay(dispatch)
            }
        }
    }

    @Test
    fun configurationRecreationDeliversPendingResultToRetainedKeyOnce() {
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            val key = PhoneControlUserStepState().projectionLauncherKey
            val first = TestRegistry()
            val oldOwner = TestOwner()
            val launcher = first.register(key, oldOwner, ActivityResultContracts.StartActivityForResult()) { }
            oldOwner.start()
            launcher.launch(Intent())
            val requestCode = first.lastRequestCode
            val saved = Bundle()
            first.onSaveInstanceState(saved)
            oldOwner.destroy()

            val restored = TestRegistry()
            restored.onRestoreInstanceState(saved)
            val newOwner = TestOwner()
            var deliveries = 0
            restored.register(key, newOwner, ActivityResultContracts.StartActivityForResult()) {
                deliveries += 1
            }
            restored.dispatchResult(requestCode, Activity.RESULT_OK, Intent())
            assertEquals(0, deliveries)
            newOwner.start()
            assertEquals(1, deliveries)
            newOwner.destroy()
        }
    }

    @Test
    fun restoredOldResultCannotReachNewProcessKey() {
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            val oldKey = PhoneControlUserStepState().projectionLauncherKey
            val first = TestRegistry()
            val oldOwner = TestOwner()
            val old = first.register(oldKey, oldOwner, ActivityResultContracts.StartActivityForResult()) { }
            oldOwner.start()
            old.launch(Intent())
            val oldCode = first.lastRequestCode
            val saved = Bundle()
            first.onSaveInstanceState(saved)
            oldOwner.destroy()

            val newKey = PhoneControlUserStepState().projectionLauncherKey
            assertNotEquals(oldKey, newKey)
            val restored = TestRegistry()
            restored.onRestoreInstanceState(saved)
            val newOwner = TestOwner()
            var deliveries = 0
            val current = restored.register(newKey, newOwner, ActivityResultContracts.StartActivityForResult()) {
                deliveries += 1
            }
            newOwner.start()
            current.launch(Intent())
            val newCode = restored.lastRequestCode
            assertNotEquals(oldCode, newCode)
            restored.dispatchResult(oldCode, Activity.RESULT_OK, Intent())
            assertEquals(0, deliveries)
            restored.dispatchResult(newCode, Activity.RESULT_OK, Intent())
            assertEquals(1, deliveries)
            newOwner.destroy()
        }
    }

    private class TestOwner : LifecycleOwner {
        private val state = LifecycleRegistry(this)
        override val lifecycle: Lifecycle get() = state
        fun start() {
            state.handleLifecycleEvent(Lifecycle.Event.ON_CREATE)
            state.handleLifecycleEvent(Lifecycle.Event.ON_START)
        }
        fun destroy() {
            state.handleLifecycleEvent(Lifecycle.Event.ON_STOP)
            state.handleLifecycleEvent(Lifecycle.Event.ON_DESTROY)
        }
    }

    private class TestRegistry : ActivityResultRegistry() {
        var lastRequestCode = -1
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

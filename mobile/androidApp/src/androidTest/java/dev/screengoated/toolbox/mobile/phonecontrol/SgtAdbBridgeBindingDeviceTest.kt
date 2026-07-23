package dev.screengoated.toolbox.mobile.phonecontrol

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbServiceClient
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SgtAdbBridgeBindingDeviceTest {
    @Test
    fun privateBridgeCanBeClosedAndReboundWithoutReusingStaleCallbacks() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext

        repeat(REBIND_ATTEMPTS) {
            SgtAdbServiceClient(context).use { client ->
                assertTrue(client.await().asBinder().pingBinder())
            }
        }
    }

    private companion object {
        const val REBIND_ATTEMPTS = 3
    }
}

package dev.screengoated.toolbox.mobile.ui

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ShellTabNavigationTest {
    @Test
    fun cancelledTabAnimation_resetsNavigating() = runTest {
        val navigation = ShellTabNavigation()
        val job = launch(start = CoroutineStart.UNDISPATCHED) {
            navigation.navigate { awaitCancellation() }
        }
        assertTrue(navigation.navigating)

        // A user swipe cancels animateScrollToPage.
        job.cancel()
        runCurrent()

        assertFalse(navigation.navigating)
    }

    @Test
    fun supersededRequestDoesNotReleaseTheNewerOne() = runTest {
        val navigation = ShellTabNavigation()
        val second = CompletableDeferred<Unit>()
        val firstJob = launch(start = CoroutineStart.UNDISPATCHED) {
            navigation.navigate { awaitCancellation() }
        }
        launch(start = CoroutineStart.UNDISPATCHED) {
            navigation.navigate { second.await() }
        }
        firstJob.cancel()
        runCurrent()
        assertTrue(navigation.navigating)

        second.complete(Unit)
        runCurrent()
        assertFalse(navigation.navigating)
    }
}

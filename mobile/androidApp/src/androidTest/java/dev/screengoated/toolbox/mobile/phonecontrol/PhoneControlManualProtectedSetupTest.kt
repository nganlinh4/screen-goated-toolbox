package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.Notification
import android.app.PendingIntent
import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCapturePolicy
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointReadiness
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupAdapter
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupNavigationContract
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupResult
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.compactPhoneControlGuidance
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlActivity
import java.util.Locale
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.awaitCancellation
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PhoneControlManualProtectedSetupTest {
    @Test
    fun manualGuidanceFitsTheOverlayForEverySupportedLocale() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        for (locale in listOf(Locale.ENGLISH, Locale.KOREAN, Locale.forLanguageTag("vi"))) {
            val config = android.content.res.Configuration(context.resources.configuration)
            config.setLocale(locale)
            val localized = context.createConfigurationContext(config)
            val message = localized.getString(R.string.phone_control_private_setup_shizuku_manual)
            assertEquals(message, compactPhoneControlGuidance(message))
        }
    }

    @Test
    fun manualInstructionsReachNotificationAndCancellationStopsOwnedWait() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val entered = CountDownLatch(1)
        val cancelled = CountDownLatch(1)
        val token = PhoneControlProtectedCheckpointRegistry.begin(
            PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION,
        )
        var notification: Notification? = null
        val adapter = object : PhoneControlProtectedSetupAdapter {
            override val capturePolicy = PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION
            override val navigationContract = PhoneControlProtectedSetupNavigationContract("test", "test")
            override val userStepGuidance = R.string.phone_control_private_setup_shizuku_manual
            override fun checkpointReadiness(context: Context) =
                PhoneControlProtectedCheckpointReadiness.Ready

            override suspend fun complete(
                context: Context,
                token: PhoneControlProtectedCheckpointToken,
            ): PhoneControlProtectedSetupResult {
                try {
                    entered.countDown()
                    awaitCancellation()
                } finally {
                    cancelled.countDown()
                }
            }
        }
        val coordinator = PhoneControlProtectedSetupCoordinator(
            context = context,
            replaceGuidance = { _, _ -> true },
            reconcileRejectedGuidance = { error("guidance should be accepted") },
            adapters = mapOf("manual" to adapter),
            publishUserStepNotification = { guidance ->
                notification = PhoneControlSetupNotification.build(context, guidance, null)
            },
        )
        try {
            instrumentation.runOnMainSync { coordinator.start("manual", token) }
            assertTrue(entered.await(5, TimeUnit.SECONDS))
            val actual = requireNotNull(notification)
            assertEquals(
                context.phoneControlString(R.string.phone_control_private_setup_shizuku_manual),
                actual.extras.getCharSequence(Notification.EXTRA_TEXT).toString(),
            )
            assertNull(actual.contentIntent)
            assertEquals(1, actual.actions.size)
            assertEquals(
                context.phoneControlString(R.string.phone_control_action_cancel_setup),
                actual.actions.single().title.toString(),
            )
            assertEquals(
                PendingIntent.getActivity(
                    context,
                    3,
                    PhoneControlActivity.cancelSetupIntent(context),
                    PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
                ),
                actual.actions.single().actionIntent,
            )
            instrumentation.runOnMainSync { coordinator.cancel() }
            assertTrue(cancelled.await(5, TimeUnit.SECONDS))
            // Cancellation does not restore visual evidence until fresh capture is granted.
            assertTrue(PhoneControlProtectedCheckpointRegistry.owns(token))
        } finally {
            instrumentation.runOnMainSync { coordinator.close() }
            PhoneControlProtectedCheckpointRegistry.end(token)
        }
    }

    @Test
    fun rejectedGuidanceReconcilesOwnedCheckpointWithoutStartingAdapter() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val token = PhoneControlProtectedCheckpointRegistry.begin(
            PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION,
        )
        var reconciled: PhoneControlProtectedCheckpointToken? = null
        var adapterStarted = false
        var notified = false
        val adapter = object : PhoneControlProtectedSetupAdapter {
            override val capturePolicy = PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION
            override val navigationContract = PhoneControlProtectedSetupNavigationContract("test", "test")
            override val userStepGuidance = R.string.phone_control_private_setup_shizuku_manual
            override fun checkpointReadiness(context: Context) =
                PhoneControlProtectedCheckpointReadiness.Ready

            override suspend fun complete(
                context: Context,
                token: PhoneControlProtectedCheckpointToken,
            ): PhoneControlProtectedSetupResult {
                adapterStarted = true
                return PhoneControlProtectedSetupResult.Completed
            }
        }
        val coordinator = PhoneControlProtectedSetupCoordinator(
            context = context,
            replaceGuidance = { _, _ -> false },
            reconcileRejectedGuidance = { reconciled = it },
            adapters = mapOf("manual" to adapter),
            publishUserStepNotification = { notified = true },
        )
        try {
            instrumentation.runOnMainSync { coordinator.start("manual", token) }
            assertEquals(token, reconciled)
            assertTrue(!adapterStarted)
            assertTrue(!notified)
            assertTrue(PhoneControlProtectedCheckpointRegistry.owns(token))
        } finally {
            instrumentation.runOnMainSync { coordinator.close() }
            PhoneControlProtectedCheckpointRegistry.end(token)
        }
    }
}

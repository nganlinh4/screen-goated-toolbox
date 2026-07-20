package dev.screengoated.toolbox.mobile.phonecontrol

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.screengoated.toolbox.mobile.MainActivity
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PhoneControlLauncherSmokeTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    @Test
    fun phoneControlIsASessionCardWithoutAnInnerLaunchSurface() {
        composeRule.onNodeWithTag("shell-tab-apps").performClick()
        composeRule.onNodeWithTag("app-card-phone-control")
            .performScrollTo()
            .assertIsDisplayed()
        composeRule.onNodeWithTag("phone-control-toggle").assertIsDisplayed()
    }
}

package dev.screengoated.toolbox.mobile.creation

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.screengoated.toolbox.mobile.MainActivity
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CreationLauncherRoutingTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun imageTo3dCardOpensItsCreationSurface() {
        assertLauncherRoute("app-card-image-to-3d", "creation-tool-3d")
    }

    @Test
    fun imageToSvgCardOpensItsCreationSurface() {
        assertLauncherRoute("app-card-image-to-svg", "creation-tool-svg")
    }

    @Test
    fun imageCreatorCardIsHiddenWhileReleaseGateIsDisabled() {
        compose.onNodeWithTag("shell-tab-apps").performClick()
        compose.onNodeWithTag("app-card-image-creator").assertDoesNotExist()
    }

    private fun assertLauncherRoute(cardTag: String, surfaceTag: String) {
        compose.onNodeWithTag("shell-tab-apps").performClick()
        compose.onNodeWithTag(cardTag)
            .performScrollTo()
            .assertIsDisplayed()
            .performClick()
        compose.waitUntil(timeoutMillis = 10_000) {
            compose.onAllNodesWithTag("creation-root")
                .fetchSemanticsNodes(atLeastOneRootRequired = false)
                .isNotEmpty()
        }
        compose.onNodeWithTag(surfaceTag).assertExists()
    }
}

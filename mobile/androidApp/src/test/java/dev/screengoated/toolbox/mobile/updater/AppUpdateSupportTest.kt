package dev.screengoated.toolbox.mobile.updater

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AppUpdateSupportTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun canonicalVersionIgnoresAndroidSuffixes() {
        assertEquals("4.8.3", canonicalAppVersion("4.8.3-full-debug"))
        assertEquals("4.8.3", canonicalAppVersion("4.8.3-play"))
    }

    @Test
    fun remoteVersionComparisonMatchesWindowsParityExpectation() {
        assertFalse(isRemoteVersionNewer("4.8.3-full-debug", "4.8.3"))
        assertTrue(isRemoteVersionNewer("4.8.2", "4.8.3"))
        assertFalse(isRemoteVersionNewer("4.8.3", "4.8.2"))
    }

    @Test
    fun sharedFixtureVersionExamplesMatchAndroidComparison() {
        val fixture = loadFixture()
        val latest = fixture.getValue("latest_release").jsonObject
        val remoteVersion = latest.getValue("tag_name").jsonPrimitive.content.removePrefix("v")
        val examples = fixture.getValue("comparison_examples").jsonArray

        examples.forEach { element ->
            val example = element.jsonObject
            val currentVersion = example.getValue("current_version").jsonPrimitive.content
            val expectedStatus = example.getValue("expected_status").jsonPrimitive.content
            val actualStatus = if (isRemoteVersionNewer(currentVersion, remoteVersion)) {
                "update_available"
            } else {
                "up_to_date"
            }
            assertEquals(expectedStatus, actualStatus)
        }
    }

    @Test
    fun androidAssetSelectionPrefersApkAndFallsBackToNull() {
        val assets = listOf(
            "ScreenGoatedToolbox_v4.8.3.exe" to "https://example.com/app.exe",
            "ScreenGoatedToolbox_v4.8.3.apk" to "https://example.com/app.apk",
        )
        assertEquals("https://example.com/app.apk", selectAndroidAssetUrl(assets))
        assertEquals(null, selectAndroidAssetUrl(listOf("a.exe" to "https://example.com/a.exe")))
    }

    @Test
    fun sharedFixtureAssetFallbackUsesReleasePageWhenApkIsMissing() {
        val fixture = loadFixture()
        val latest = fixture.getValue("latest_release").jsonObject
        val assets = latest.getValue("assets").jsonArray.map { element ->
            val asset = element.jsonObject
            asset.getValue("name").jsonPrimitive.content to
                asset.getValue("browser_download_url").jsonPrimitive.content
            }
        val state = AppUpdateUiState(
            status = AppUpdateStatus.UPDATE_AVAILABLE,
            currentVersion = "4.8.2",
            latestVersion = latest.getValue("tag_name").jsonPrimitive.content.removePrefix("v"),
            releaseNotes = latest.getValue("body").jsonPrimitive.content,
            releaseUrl = latest.getValue("html_url").jsonPrimitive.content,
            assetUrl = selectAndroidAssetUrl(assets),
        )

        val assetSelection = fixture.getValue("asset_selection").jsonObject
        assertEquals(".apk", assetSelection.getValue("preferred_extension").jsonPrimitive.content)
        assertEquals("release_html_url", assetSelection.getValue("fallback_when_missing").jsonPrimitive.content)
        assertEquals(null, state.assetUrl)
        assertEquals(latest.getValue("html_url").jsonPrimitive.content, state.actionUrl)
    }

    @Test
    fun startupAutoCheckRunsOncePerAppLaunch() {
        val repositorySource = File(repoRoot(), APP_UPDATE_REPOSITORY_SOURCE).readText()
        val viewModelSource = File(repoRoot(), MAIN_VIEW_MODEL_SOURCE).readText()

        assertTrue(repositorySource.contains("private var autoCheckStarted = false"))
        assertTrue(repositorySource.contains("fun autoCheckForUpdates()"))
        assertTrue(repositorySource.contains("if (autoCheckStarted)"))
        assertTrue(repositorySource.contains("autoCheckStarted = true"))
        assertTrue(viewModelSource.contains("appUpdateController.autoCheckForUpdates()"))
    }

    private fun loadFixture(): JsonObject {
        return json.parseToJsonElement(File(repoRoot(), FIXTURE_PATH).readText()).jsonObject
    }

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/app-update/latest-release.json"
        private const val APP_UPDATE_REPOSITORY_SOURCE =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/updater/AppUpdateRepository.kt"
        private const val MAIN_VIEW_MODEL_SOURCE =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/MainViewModel.kt"
    }
}

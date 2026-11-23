package dev.screengoated.toolbox.mobile.updater

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AppUpdateSupportTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun canonicalVersionIgnoresAndroidSuffixes() {
        assertEquals("5.5.0", canonicalAppVersion("5.5.0-full-debug"))
        assertEquals("5.5.0", canonicalAppVersion("5.5.0-play"))
    }

    @Test
    fun sharedFixtureVersionExamplesMatchAndroidComparison() {
        val fixture = loadFixture()
        val remoteVersion = fixture.getValue("latest_release").jsonObject
            .getValue("tag_name").jsonPrimitive.content.removePrefix("v")
        fixture.getValue("comparison_examples").jsonArray.forEach { element ->
            val example = element.jsonObject
            val currentVersion = example.getValue("current_version").jsonPrimitive.content
            val actual = if (isRemoteVersionNewer(currentVersion, remoteVersion)) {
                "update_available"
            } else {
                "up_to_date"
            }
            assertEquals(example.getValue("expected_status").jsonPrimitive.content, actual)
        }
    }

    @Test
    fun stableManifestSelectsExactFullApkContract() {
        val fixture = loadFixture()
        val payload = fixture.getValue("stable_manifest").toString().encodeToByteArray()
        val candidate = stableManifestCandidate(payload)

        assertEquals("9.9.9", candidate.version)
        assertEquals("ScreenGoatedToolbox_v9.9.9.apk", candidate.assetName)
        assertEquals(98_765_432L, candidate.sizeBytes)
        assertEquals("2".repeat(64), candidate.sha256)
        assertEquals(
            "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/" +
                "v9.9.9/ScreenGoatedToolbox_v9.9.9.apk",
            candidate.assetUrl,
        )
    }

    @Test
    fun githubFallbackSelectsOnlyUnambiguousStableFullApk() {
        val release = JSONObject(loadFixture().getValue("latest_release").toString())
        val candidate = githubCandidates(JSONArray().put(release).toString().encodeToByteArray()).single()

        assertEquals("9.9.9", candidate.version)
        assertEquals("ScreenGoatedToolbox_v9.9.9.apk", candidate.assetName)
    }

    @Test
    fun githubFallbackRejectsDraftPrereleaseStagingAndMalformedAssets() {
        val original = JSONObject(loadFixture().getValue("latest_release").toString())
        val rejected = listOf(
            JSONObject(original.toString()).put("draft", true),
            JSONObject(original.toString()).put("prerelease", true),
            JSONObject(original.toString()).put("tag_name", "sgt-runtime-staging"),
            JSONObject(original.toString()).apply {
                getJSONArray("assets").getJSONObject(1).remove("digest")
            },
            JSONObject(original.toString()).apply {
                val assets = getJSONArray("assets")
                assets.put(JSONObject(assets.getJSONObject(1).toString()))
            },
            JSONObject(original.toString()).apply {
                getJSONArray("assets").getJSONObject(1).put("size", 0)
            },
        )

        rejected.forEach { release ->
            assertTrue(githubCandidates(JSONArray().put(release).toString().encodeToByteArray()).isEmpty())
        }
    }

    @Test(expected = IllegalArgumentException::class)
    fun stableManifestRejectsVersionComponentsOutsideWindowsSemverRange() {
        val root = JSONObject(loadFixture().getValue("stable_manifest").toString())
        val version = "18446744073709551616.0.0"
        root.put("version", version)
        root.getJSONObject("androidFullApk")
            .put("name", "ScreenGoatedToolbox_v$version.apk")
            .put(
                "url",
                "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/" +
                    "v$version/ScreenGoatedToolbox_v$version.apk",
            )
        stableManifestCandidate(root.toString().encodeToByteArray())
    }

    @Test
    fun startupAutoCheckRunsOncePerAppLaunch() {
        val owner=AppUpdateStateOwner<Unit>("1.0.0")
        val check=requireNotNull(owner.beginAutoCheck())
        owner.finishCheck(check,AppUpdateProjection(AppUpdateStatus.UP_TO_DATE))
        assertEquals(null,owner.beginAutoCheck())
        assertTrue(owner.beginCheck()!=null)
    }

    private fun loadFixture(): JsonObject =
        json.parseToJsonElement(File(repoRoot(), FIXTURE_PATH).readText()).jsonObject

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root -> File(root, FIXTURE_PATH).exists() }
            ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private companion object {
        const val FIXTURE_PATH = "parity-fixtures/app-update/latest-release.json"
    }
}

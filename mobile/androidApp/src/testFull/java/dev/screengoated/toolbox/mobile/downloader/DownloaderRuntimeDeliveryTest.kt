package dev.screengoated.toolbox.mobile.downloader

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class DownloaderRuntimeDeliveryTest {
    @Test
    fun releaseManifestPinsEveryDownloadedByte() {
        val manifest = repoFile("mobile/androidApp/delivery/downloader-runtime.json").readText()
        val delivery = parseDownloaderRuntimeDelivery(manifest)

        assertEquals("2026.08.19-android-0.18.2", delivery.version)
        assertEquals(DownloaderArtifactRole.entries.toSet(), delivery.artifacts.map { it.role }.toSet())
        assertEquals(
            3_072_469L,
            delivery.artifact(DownloaderArtifactRole.YT_DLP).sizeBytes,
        )
        assertEquals(
            "1fa6733c37ea6fb51c99ad8fe785e7b7e5f3246c9b980230329d4fb72ed8d4d6",
            delivery.artifact(DownloaderArtifactRole.YT_DLP).sha256,
        )
        delivery.artifacts.forEach {
            assertEquals(64, it.sha256.length)
            assertFalse(it.downloadUrl.contains("latest", ignoreCase = true))
            assertFalse(it.downloadUrl.contains("nightly", ignoreCase = true))
            assertFalse(it.downloadUrl.contains("master", ignoreCase = true))
        }
    }

    @Test
    fun mutableOfficialReleaseUrlIsRejected() {
        val valid = repoFile("mobile/androidApp/delivery/downloader-runtime.json").readText()
        val mutable = valid.replace(
            "releases/download/2026.08.19/yt-dlp",
            "releases/latest/download/yt-dlp",
        )

        assertThrows(IllegalArgumentException::class.java) {
            parseDownloaderRuntimeDelivery(mutable)
        }
    }

    @Test
    fun archiveOutsideRuntimeBundlesIsRejected() {
        val valid = repoFile("mobile/androidApp/delivery/downloader-runtime.json").readText()
        val unowned = valid.replace(
            "github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/" +
                "sgt-downloader-python",
            "example.com/sgt-downloader-python",
        )

        assertThrows(IllegalArgumentException::class.java) {
            parseDownloaderRuntimeDelivery(unowned)
        }
    }

    @Test
    fun stagingRuntimeIsAcceptedOnlyWhenExplicitlyAllowed() {
        val valid = repoFile("mobile/androidApp/delivery/downloader-runtime.json").readText()
        val staging = valid.replace(
            "/releases/download/sgt-runtime-bundles/",
            "/releases/download/sgt-runtime-staging/",
        )

        assertThrows(IllegalArgumentException::class.java) {
            parseDownloaderRuntimeDelivery(staging)
        }
        assertEquals(
            "2026.08.19-android-0.18.2",
            parseDownloaderRuntimeDelivery(staging, allowStaging = true).version,
        )
    }

    @Test
    fun installedPathsCannotEscapeTheirComponent() {
        assertTrue(isSafeRelativePath("usr/lib/libpython3.11.so.1.0"))
        assertFalse(isSafeRelativePath("../libpython.so"))
        assertFalse(isSafeRelativePath("usr\\lib\\python.so"))
        assertFalse(isSafeRelativePath("/data/local/tmp/python.so"))
    }

    @Test
    fun fullBaseHasNoDownloaderJavaApiOrMutableUpdater() {
        val sourceRoot = repoFile(
            "mobile/androidApp/src/full/java/dev/screengoated/toolbox/mobile/downloader",
        )
        val source = sourceRoot.walkTopDown()
            .filter { it.extension == "kt" }
            .joinToString("\n") { it.readText() }
        val gradle = listOf(
            "mobile/androidApp/build.gradle.kts",
            "mobile/androidApp/gradle/runtime-delivery.gradle.kts",
        ).joinToString("\n") { repoFile(it).readText() }

        assertFalse(source.contains("import com.yausername"))
        assertFalse(source.contains("YoutubeDL"))
        assertFalse(source.contains("UpdateChannel"))
        assertFalse(source.contains("releases/latest"))
        assertFalse(gradle.contains("\"fullRuntimeOnly\"(libs.youtubedl.android.library)"))
        assertFalse(gradle.contains("\"fullRuntimeOnly\"(libs.youtubedl.android.ffmpeg)"))
        assertFalse(gradle.contains("\"fullImplementation\"(libs.youtubedl.android.library)"))
        assertFalse(gradle.contains("\"fullImplementation\"(libs.youtubedl.android.ffmpeg)"))
        assertTrue(gradle.contains("stageFullDownloaderLaunchers"))
        assertTrue(gradle.contains("generatedFullDownloaderLauncherJniLibs"))
    }
}

private fun repoFile(relativePath: String): File {
    var current = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
    while (true) {
        val candidate = File(current, relativePath)
        if (candidate.exists()) return candidate
        current = current.parentFile ?: error("Could not find $relativePath")
    }
}

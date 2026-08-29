package dev.screengoated.toolbox.mobile.downloader

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class DownloaderRealUrlInstrumentedTest {
    @Test
    fun selectedPublicUrlCanBeAnalyzedByTheInstalledRuntime() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val url = InstrumentationRegistry.getArguments().getString("videoUrl").orEmpty()
        assumeTrue("Pass -e videoUrl to run the real downloader probe", url.isNotBlank())
        val context = instrumentation.targetContext
        val delivery = requireNotNull(loadDownloaderRuntimeDelivery(context))
        val installer = DownloaderRuntimeInstaller(context, delivery, okhttp3.OkHttpClient())
        installer.install { }
        val ffmpeg = java.io.File(context.applicationInfo.nativeLibraryDir, "libffmpeg.so")
        val ffmpegLibraryDirectory = java.io.File(installer.ffmpegDirectory, "usr/lib")
        val ffmpegProcess = ProcessBuilder(ffmpeg.absolutePath, "-version").apply {
            environment()["SGT_FFMPEG_LIBRARY_DIR"] = ffmpegLibraryDirectory.absolutePath
        }.start()
        val ffmpegOut = ffmpegProcess.inputStream.bufferedReader().readText()
        val ffmpegError = ffmpegProcess.errorStream.bufferedReader().readText()
        assertEquals(ffmpegError, 0, ffmpegProcess.waitFor())
        assertTrue(ffmpegError, ffmpegOut.contains("ffmpeg version"))
        val host = DownloaderProcessHost(context, installer) { AutoCloseable { } }
        val request = YtDlpCommand(url).apply {
            addOption("--dump-json")
            addOption("--no-download")
            addOption("--no-playlist")
        }

        val response = host.execute(request, "instrumented-analysis")
        val result = JSONObject(response.out)

        assertEquals(0, response.exitCode)
        assertTrue(result.getJSONArray("formats").length() > 0)
        assertTrue(result.getString("id").isNotBlank())

        val outputDirectory = context.cacheDir.resolve("downloader-real-url-probe")
        outputDirectory.deleteRecursively()
        assertTrue(outputDirectory.mkdirs())
        try {
            val download = YtDlpCommand(url).apply {
                addOption("--no-playlist")
                addOption("--download-sections", "*0-1")
                addOption("--force-keyframes-at-cuts")
                addOption("-f", "bestvideo+bestaudio/best")
                addOption("-o", outputDirectory.resolve("%(id)s.%(ext)s").absolutePath)
            }
            assertEquals(0, host.execute(download, "instrumented-download").exitCode)
            assertTrue(outputDirectory.listFiles().orEmpty().any { it.isFile && it.length() > 0L })
        } finally {
            outputDirectory.deleteRecursively()
        }
    }
}

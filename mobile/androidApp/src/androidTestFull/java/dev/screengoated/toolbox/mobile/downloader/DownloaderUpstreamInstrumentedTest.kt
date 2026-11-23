package dev.screengoated.toolbox.mobile.downloader

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.BuildConfig
import java.io.IOException
import java.nio.file.Files
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class DownloaderUpstreamInstrumentedTest {
    @Test
    fun existingRuntimeUpdatesAndDownloadsUsingOnlyUpstreamDiscovery() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val arguments = InstrumentationRegistry.getArguments()
        val url = arguments.getString("upstreamTestVideo").orEmpty()
        assumeTrue("Pass -e upstreamTestVideo on a seeded debug installation", url.isNotBlank())
        val context = instrumentation.targetContext
        assertTrue(BuildConfig.DEBUG)

        val delivery = context.assets.open("downloader-runtime/delivery.json").bufferedReader().use {
            parseDownloaderRuntimeDelivery(it.readText())
        }
        val upstreamAvailable = AtomicBoolean(true)
        val client = OkHttpClient.Builder().addInterceptor { chain ->
            val request = chain.request()
            val path = request.url.encodedPath
            check(
                request.url.host == "api.github.com" && path == "/repos/yt-dlp/yt-dlp/releases/latest" ||
                    request.url.host == "github.com" && path.startsWith("/yt-dlp/yt-dlp/releases/download/"),
            ) { "The updater requested an unexpected release source" }
            if (!upstreamAvailable.get()) throw IOException("Test upstream connection unavailable")
            chain.proceed(request)
        }.build()
        val upstream = UpstreamYtDlp(client) { context.assets.open("downloader-runtime/yt-dlp-release-key.asc") }
        val repo = DownloaderRepository(context, DownloaderPersistence(context, Json), delivery, upstream)
        val installer = requireNotNull(repo.runtimeInstaller)
        val store = requireNotNull(repo.ytDlpStore)
        val host = requireNotNull(repo.processHost)
        assertTrue("The prior release's verified runtime must already be installed", installer.isInstalled())
        val nativeFiles = listOf(
            installer.pythonDirectory.resolve("usr/lib/libpython3.11.so.1.0"),
            installer.ffmpegDirectory.resolve("usr/lib/libffmpeg_real.so"),
        )
        val nativeTimestamps = nativeFiles.map { it.lastModified() }
        val root = Files.createTempDirectory(context.noBackupFilesDir.toPath(), "upstream-probe-").toFile()
        val hiddenBootstrap = root.resolve("bootstrap")
        try {
            val before = store.acquire()
            before.use {
                assertEquals(installer.ytdlpFile, before.tool.executable)
                repo.checkTools()
                val state = withTimeout(90_000) {
                    repo.state.first { it.ytDlpUpdate in setOf(YtDlpUpdateStatus.UPDATED, YtDlpUpdateStatus.FAILED) }
                }
                assertEquals(YtDlpUpdateStatus.UPDATED, state.ytDlpUpdate)
                assertTrue(store.current().version > before.tool.version)
                assertEquals(installer.ytdlpFile, before.tool.executable)
                assertTrue(before.tool.executable.isFile)
                assertEquals(nativeTimestamps, nativeFiles.map { it.lastModified() })
            }
            val current = store.current()
            Files.move(installer.ytdlpFile.toPath(), hiddenBootstrap.toPath())
            upstreamAvailable.set(false)
            val reopened = DownloaderRepository(context, DownloaderPersistence(context, Json), delivery, upstream)
            assertEquals(current, reopened.ytDlpStore?.current())
            reopened.checkTools()
            withTimeout(10_000) { reopened.state.first { it.toolsReady } }
            withContext(Dispatchers.Main) { reopened.checkYtDlpForUpdates() }
            val offline = withTimeout(20_000) { reopened.state.first { it.ytDlpUpdate == YtDlpUpdateStatus.FAILED } }
            assertTrue(offline.toolsReady)
            assertEquals(current, reopened.ytDlpStore?.current())
            upstreamAvailable.set(true)

            val request = YtDlpCommand(url).apply {
                addOption("--no-playlist")
                addOption("--max-filesize", "10M")
                addOption("-o", root.resolve("download.%(ext)s").absolutePath)
            }
            assertEquals(0, host.execute(request, "upstream-runtime-download").exitCode)
            assertTrue(root.listFiles().orEmpty().any { it.name.startsWith("download.") && it.length() > 0L })
        } finally {
            if (hiddenBootstrap.exists()) Files.move(hiddenBootstrap.toPath(), installer.ytdlpFile.toPath())
            safeDeleteTree(context.noBackupFilesDir, root)
        }
    }
}

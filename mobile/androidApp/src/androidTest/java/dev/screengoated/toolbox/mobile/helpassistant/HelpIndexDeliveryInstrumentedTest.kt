package dev.screengoated.toolbox.mobile.helpassistant

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.io.IOException

@RunWith(AndroidJUnit4::class)
class HelpIndexDeliveryInstrumentedTest {
    @Test
    fun selectedAssetDownloadsVerifiesAndReusesOffline() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        val cache = File(context.filesDir, "help-assistant")
        cache.deleteRecursively()

        val delivery = context.assets.open("help-assistant/delivery.json").use { input ->
            parseDelivery(input.readBytes())
        }
        assertTrue(
            delivery.downloadUrl.contains("/sgt-runtime-staging/") ||
                delivery.downloadUrl.contains("/sgt-runtime-bundles/"),
        )

        val downloaded = HelpIndexStore(context, OkHttpClient()).load()
        assertEquals(listOf("docs/help/shared-workflows.md", "docs/help/android-guide.md"), downloaded.map(ChunkEntry::path))
        assertTrue(File(cache, delivery.asset).isFile)
        assertTrue(File(cache, "last-good.json").isFile)
        assertTrue(File(cache, "last-good.sha256").isFile)

        val offline = OkHttpClient.Builder()
            .addInterceptor { throw IOException("network must not be used for a verified cache hit") }
            .build()
        val cached = HelpIndexStore(context, offline).load()
        assertEquals(downloaded, cached)

        assertTrue(File(cache, delivery.asset).delete())
        val lastGood = HelpIndexStore(context, offline).load()
        assertEquals(downloaded, lastGood)
    }
}

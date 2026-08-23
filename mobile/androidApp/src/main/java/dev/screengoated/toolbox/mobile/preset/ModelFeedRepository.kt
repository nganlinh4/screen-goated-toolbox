package dev.screengoated.toolbox.mobile.preset

import android.content.Context
import android.util.Base64
import java.io.File
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Request
import okio.Buffer
import org.json.JSONObject

internal class ModelFeedRepository(
    context: Context,
    private val httpClient: OkHttpClient,
) {
    private val appContext = context.applicationContext
    private val cacheFile = File(appContext.noBackupFilesDir, "model-feed/nvidia-availability.cache")
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val started = AtomicBoolean(false)
    private val publicPoint by lazy {
        val value = appContext.assets.open(PUBLIC_KEY_ASSET).bufferedReader().use { it.readText() }
        decodeFeedPublicKey(value.trim())
    }

    fun start() {
        if (!started.compareAndSet(false, true)) return
        loadCache()?.let(PresetModelFeed::publish)
        scope.launch {
            while (isActive) {
                if (cacheIsStale()) {
                    runCatching(::refresh).onFailure { error ->
                        android.util.Log.i(LOG_TAG, "Availability refresh skipped", error)
                    }
                }
                delay(REFRESH_INTERVAL_MILLIS)
            }
        }
    }

    internal fun refresh(): AvailabilityFeed {
        val payload = fetch(FEED_URL, MAXIMUM_FEED_BYTES)
        val signature = fetch(SIGNATURE_URL, SIGNATURE_BYTES)
        require(signature.size == SIGNATURE_BYTES)
        val feed = parseVerifiedAvailabilityFeed(publicPoint, payload, signature)
        writeCache(payload, signature)
        PresetModelFeed.publish(feed)
        android.util.Log.i(
            LOG_TAG,
            "${feed.provider} availability refreshed, ${rankedFeedModels(feed).size} offered.",
        )
        return feed
    }

    private fun loadCache(): AvailabilityFeed? = runCatching {
        if (!cacheFile.isFile || cacheFile.length() > MAXIMUM_CACHE_BYTES) return@runCatching null
        val root = JSONObject(cacheFile.readText())
        val payload = Base64.decode(root.getString("payload"), Base64.DEFAULT)
        val signature = Base64.decode(root.getString("signature"), Base64.DEFAULT)
        parseVerifiedAvailabilityFeed(publicPoint, payload, signature)
    }.getOrNull()

    private fun writeCache(payload: ByteArray, signature: ByteArray) {
        val directory = requireNotNull(cacheFile.parentFile)
        check(directory.isDirectory || directory.mkdirs())
        val content = JSONObject()
            .put("payload", Base64.encodeToString(payload, Base64.NO_WRAP))
            .put("signature", Base64.encodeToString(signature, Base64.NO_WRAP))
            .toString()
            .toByteArray()
        val temporary = File(directory, "${cacheFile.name}.tmp")
        temporary.writeBytes(content)
        try {
            Files.move(
                temporary.toPath(),
                cacheFile.toPath(),
                StandardCopyOption.REPLACE_EXISTING,
                StandardCopyOption.ATOMIC_MOVE,
            )
        } catch (_: AtomicMoveNotSupportedException) {
            Files.move(
                temporary.toPath(),
                cacheFile.toPath(),
                StandardCopyOption.REPLACE_EXISTING,
            )
        }
    }

    private fun fetch(url: String, limit: Int): ByteArray {
        val request = Request.Builder().url(url).get().build()
        val call = httpClient.newCall(request)
        call.timeout().timeout(FETCH_TIMEOUT_SECONDS, java.util.concurrent.TimeUnit.SECONDS)
        call.execute().use { response ->
            check(response.isSuccessful) { "Availability fetch failed with HTTP ${response.code}" }
            require(response.body.contentLength() <= limit || response.body.contentLength() == -1L) {
                "Availability response is larger than expected"
            }
            val source = response.body.source()
            val buffer = Buffer()
            while (buffer.size <= limit) {
                val remaining = limit.toLong() + 1L - buffer.size
                if (source.read(buffer, minOf(8_192L, remaining)) == -1L) break
            }
            val bytes = buffer.readByteArray()
            require(bytes.size <= limit) { "Availability response is larger than expected" }
            return bytes
        }
    }

    private fun cacheIsStale(): Boolean = !cacheFile.isFile ||
        System.currentTimeMillis() - cacheFile.lastModified() >= REFRESH_INTERVAL_MILLIS

    private companion object {
        const val FEED_URL =
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/monitoring-feed/nvidia-availability.json"
        const val SIGNATURE_URL =
            "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/monitoring-feed/nvidia-availability.json.sig"
        const val PUBLIC_KEY_ASSET = "model-feed/public-key.hex"
        const val REFRESH_INTERVAL_MILLIS = 15L * 60L * 1_000L
        const val MAXIMUM_CACHE_BYTES = 512L * 1_024L
        const val FETCH_TIMEOUT_SECONDS = 10L
        const val LOG_TAG = "SgtModelFeed"
    }
}

package dev.screengoated.toolbox.mobile.preset

import kotlinx.coroutines.Job
import okhttp3.Call
import okhttp3.Response
import okhttp3.ResponseBody
import okio.Buffer
import okio.ForwardingSource
import okio.buffer
import java.io.IOException
import java.net.SocketTimeoutException
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.ScheduledThreadPoolExecutor
import java.util.concurrent.TimeUnit

/** Owned by one call, never by its pooled connection. Only decoded output ends the deadline. */
internal class PresetFirstTokenDeadline(
    private val budgetMillis: Long,
    private val job: Job?,
) {
    private var startedNanos: Long? = null
    private var received = false
    private var finished = false
    private var expired = false
    private var monitor: ScheduledFuture<*>? = null

    @Synchronized
    fun attach(call: Call) {
        monitor = scheduler.scheduleWithFixedDelay({ check(call) }, 0, 25, TimeUnit.MILLISECONDS)
    }

    @Synchronized
    fun start() {
        if (startedNanos == null) startedNanos = System.nanoTime()
    }

    @Synchronized
    fun received() {
        received = true
    }

    @Synchronized
    private fun check(call: Call) {
        if (finished) return
        val started = startedNanos
        if (!received && started != null &&
            System.nanoTime() - started >= TimeUnit.MILLISECONDS.toNanos(budgetMillis)
        ) {
            expired = true
            call.cancel()
        } else if (job?.isActive == false) {
            call.cancel()
        }
    }

    @Synchronized
    fun close() {
        finished = true
        monitor?.cancel(false)
        monitor = null
    }

    @Synchronized
    fun failure(error: IOException): IOException = if (expired) {
        SocketTimeoutException("Timed out waiting for first output token").apply { initCause(error) }
    } else {
        error
    }

    fun wrap(body: ResponseBody): ResponseBody {
        val source = object : ForwardingSource(body.source()) {
            override fun read(sink: Buffer, byteCount: Long): Long = try {
                super.read(sink, byteCount).also { if (it == -1L) closeDeadline() }
            } catch (error: IOException) {
                closeDeadline()
                throw failure(error)
            }

            override fun close() {
                closeDeadline()
                super.close()
            }
        }.buffer()
        return object : ResponseBody() {
            override fun contentType() = body.contentType()
            override fun contentLength() = body.contentLength()
            override fun source() = source
        }
    }

    private fun closeDeadline() = close()

    companion object {
        private val scheduler = ScheduledThreadPoolExecutor(1) { runnable ->
            Thread(runnable, "preset-response-deadline").apply { isDaemon = true }
        }.apply { removeOnCancelPolicy = true }
    }
}

internal fun Response.firstOutputReceived() {
    request.tag(PresetFirstTokenDeadline::class.java)?.received()
}

package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

import ai.onnxruntime.OnnxTensor
import ai.onnxruntime.OrtEnvironment
import ai.onnxruntime.OrtSession
import android.graphics.Bitmap
import java.io.File
import java.nio.FloatBuffer
import kotlin.system.measureTimeMillis
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

internal data class UiDetectorInference(
    val output: UiDetectorOutput,
    val durationMs: Long,
    val executionProvider: String,
)

/** Serialized CPU baseline. XNNPACK/NNAPI remain benchmark choices, not device-name guesses. */
internal object UiDetectorOnnxEngine {
    private val mutex = Mutex()
    private var session: OrtSession? = null
    private var sessionModelIdentity: Pair<String, Long>? = null

    suspend fun detect(
        bitmap: Bitmap,
        originX: Int,
        originY: Int,
        model: File,
    ): UiDetectorInference = withContext(Dispatchers.Default) {
        mutex.withLock {
            var output: UiDetectorOutput? = null
            val duration = measureTimeMillis {
                val activeSession = sessionFor(model)
                val tensorData = preprocess(bitmap)
                val environment = OrtEnvironment.getEnvironment()
                OnnxTensor.createTensor(
                    environment,
                    FloatBuffer.wrap(tensorData),
                    longArrayOf(
                        1L,
                        3L,
                        UiDetectorContract.INPUT_SIDE.toLong(),
                        UiDetectorContract.INPUT_SIDE.toLong(),
                    ),
                ).use { input ->
                    activeSession.run(mapOf("input" to input)).use { results ->
                        val dets = results.get("dets").orElseThrow {
                            IllegalStateException("UI detector model has no dets output")
                        } as? OnnxTensor ?: error("UI detector dets output is not a tensor")
                        val labels = results.get("labels").orElseThrow {
                            IllegalStateException("UI detector model has no labels output")
                        } as? OnnxTensor ?: error("UI detector labels output is not a tensor")
                        output = postprocessUiDetector(
                            detsShape = dets.info.shape,
                            dets = dets.floatBuffer.toArray(),
                            labelsShape = labels.info.shape,
                            labels = labels.floatBuffer.toArray(),
                            cropWidth = bitmap.width,
                            cropHeight = bitmap.height,
                            originX = originX,
                            originY = originY,
                        )
                    }
                }
            }
            UiDetectorInference(
                output = requireNotNull(output),
                durationMs = duration,
                executionProvider = "cpu",
            )
        }
    }

    private fun sessionFor(model: File): OrtSession {
        val identity = model.absolutePath to model.lastModified()
        if (sessionModelIdentity == identity) return requireNotNull(session)
        session?.close()
        val environment = OrtEnvironment.getEnvironment()
        session = OrtSession.SessionOptions().use { options ->
            options.setOptimizationLevel(OrtSession.SessionOptions.OptLevel.ALL_OPT)
            environment.createSession(model.absolutePath, options)
        }
        sessionModelIdentity = identity
        return requireNotNull(session)
    }

    private fun preprocess(source: Bitmap): FloatArray {
        require(source.width > 0 && source.height > 0) { "UI detector bitmap is empty" }
        val side = UiDetectorContract.INPUT_SIDE
        val resized = Bitmap.createScaledBitmap(source, side, side, true)
        try {
            val pixels = IntArray(side * side)
            resized.getPixels(pixels, 0, side, 0, 0, side, side)
            val plane = side * side
            val chw = FloatArray(plane * 3)
            pixels.forEachIndexed { index, pixel ->
                val red = ((pixel ushr 16) and 0xff) / 255f
                val green = ((pixel ushr 8) and 0xff) / 255f
                val blue = (pixel and 0xff) / 255f
                chw[index] = (red - UiDetectorContract.MEAN[0]) / UiDetectorContract.STD[0]
                chw[plane + index] =
                    (green - UiDetectorContract.MEAN[1]) / UiDetectorContract.STD[1]
                chw[plane * 2 + index] =
                    (blue - UiDetectorContract.MEAN[2]) / UiDetectorContract.STD[2]
            }
            return chw
        } finally {
            if (resized !== source) resized.recycle()
        }
    }
}

private fun FloatBuffer.toArray(): FloatArray {
    val copy = duplicate()
    copy.rewind()
    return FloatArray(copy.remaining()).also { copy.get(it) }
}

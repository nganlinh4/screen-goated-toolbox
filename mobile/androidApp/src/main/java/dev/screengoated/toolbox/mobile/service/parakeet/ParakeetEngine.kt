package dev.screengoated.toolbox.mobile.service.parakeet

import ai.onnxruntime.OnnxTensor
import ai.onnxruntime.OrtEnvironment
import ai.onnxruntime.OrtSession
import android.util.Log
import java.io.File
import java.nio.FloatBuffer
import java.nio.IntBuffer
import java.nio.LongBuffer
import java.util.ArrayDeque

/**
 * Kotlin port of parakeet-rs ParakeetEOU for streaming ASR.
 * Uses ONNX Runtime Android (CPU/NNAPI) with identical model files.
 */
internal class ParakeetEngine(modelDir: File, ortLibDir: File? = null) {

    private val env: OrtEnvironment

    init {
        // Pre-load ORT native libs from downloaded location before OrtEnvironment
        // triggers System.loadLibrary("onnxruntime") which would fail without them.
        if (ortLibDir != null) {
            val ort = File(ortLibDir, "libonnxruntime.so")
            val jni = File(ortLibDir, "libonnxruntime4j_jni.so")
            if (ort.exists()) System.load(ort.absolutePath)
            if (jni.exists()) System.load(jni.absolutePath)
        }
        env = OrtEnvironment.getEnvironment()
    }
    private val encoderSession: OrtSession
    private val decoderSession: OrtSession
    private val tokenizer: SentencePieceTokenizer
    private val melSpectrogram = MelSpectrogram()

    // Encoder cache: [17, 1, 70, 512] channel, [17, 1, 512, 8] time, [1] len
    private var cacheChannel = FloatArray(17 * 1 * 70 * 512)
    private var cacheTime = FloatArray(17 * 1 * 512 * 8)
    private var cacheLen = longArrayOf(0L)

    // Decoder LSTM states: [1, 1, 640]
    private var stateH = FloatArray(640)
    private var stateC = FloatArray(640)
    private var lastToken: Int

    // 4-second ring buffer at 16kHz
    private val audioBuffer = ArrayDeque<Float>(BUFFER_SIZE)

    init {
        tokenizer = SentencePieceTokenizer(File(modelDir, "tokenizer.json"))
        lastToken = tokenizer.blankId

        // XNNPACK EP uses optimized ARM NEON kernels — significantly faster than default CPU
        // on ARM64 devices. Falls back to CPU for unsupported ops without the overhead that
        // NNAPI has (NNAPI does expensive per-op delegation even on fallback).
        val cpuCores = Runtime.getRuntime().availableProcessors()
        val xnnpackThreads = cpuCores.coerceIn(2, 4)
        val opts = OrtSession.SessionOptions().apply {
            // ORT intra-op threadpool size = 1 to avoid contention with XNNPACK's pthread pool
            setIntraOpNumThreads(1)
            setInterOpNumThreads(1)
            addConfigEntry("session.intra_op.allow_spinning", "0")
            try {
                addXnnpack(mapOf("intra_op_num_threads" to xnnpackThreads.toString()))
                Log.d(TAG, "XNNPACK enabled ($xnnpackThreads threads)")
            } catch (e: Exception) {
                // Fallback: let ORT CPU use multiple threads if XNNPACK unavailable
                setIntraOpNumThreads(xnnpackThreads)
                Log.d(TAG, "XNNPACK not available, CPU fallback ($xnnpackThreads threads): ${e.message}")
            }
        }

        // Prefer INT8 quantized models (~2x faster) if available, fall back to fp32
        val encoderFile = File(modelDir, "encoder.int8.onnx").let { if (it.exists()) it else File(modelDir, "encoder.onnx") }
        val decoderFile = File(modelDir, "decoder_joint.int8.onnx").let { if (it.exists()) it else File(modelDir, "decoder_joint.onnx") }
        val usingInt8 = encoderFile.name.contains("int8")

        Log.d(TAG, "Loading ONNX sessions ($cpuCores cores, int8=$usingInt8)")

        encoderSession = env.createSession(encoderFile.absolutePath, opts)
        decoderSession = env.createSession(decoderFile.absolutePath, opts)

        Log.d(TAG, "ParakeetEngine loaded (vocab=${tokenizer.vocabSize}, blank=${tokenizer.blankId}, eou=${tokenizer.eouId}, int8=$usingInt8)")
    }

    /**
     * Process a chunk of 16kHz mono audio (typically 2560 samples = 160ms).
     * Returns transcribed text for this chunk, or empty string if nothing recognized yet.
     */
    private var chunkCount = 0L
    private var totalMelMs = 0L
    private var totalEncMs = 0L
    private var totalDecMs = 0L

    fun transcribe(samples: FloatArray): String {
        // Add to ring buffer
        for (s in samples) audioBuffer.addLast(s)
        while (audioBuffer.size > BUFFER_SIZE) audioBuffer.removeFirst()

        // Need at least 1 second of audio
        if (audioBuffer.size < SAMPLE_RATE) return ""

        chunkCount++
        var t0 = System.nanoTime()

        // Only extract mel features for the audio we need, not the full 4-second buffer.
        // We need SLICE_LEN mel frames. Each frame uses HOP_LENGTH=160 samples, plus
        // WIN_LENGTH=400 context. Add extra margin for padding.
        val melAudioNeeded = (SLICE_LEN + 4) * MelSpectrogram.HOP_LENGTH + MelSpectrogram.WIN_LENGTH + MelSpectrogram.N_FFT
        val audioLen = audioBuffer.size
        val melStart = (audioLen - melAudioNeeded).coerceAtLeast(0)
        val melSamples = audioLen - melStart

        val bufferArray = FloatArray(melSamples)
        val iter = audioBuffer.iterator()
        // Skip to melStart
        repeat(melStart) { iter.next() }
        for (i in 0 until melSamples) bufferArray[i] = iter.next()

        val features = melSpectrogram.extract(bufferArray)
        val melMs = (System.nanoTime() - t0) / 1_000_000
        totalMelMs += melMs
        val totalFrames = features.numFrames

        // Take last SLICE_LEN frames for encoder
        val startFrame = (totalFrames - SLICE_LEN).coerceAtLeast(0)
        val sliceFrames = totalFrames - startFrame

        // Build encoder input: [1, 128, sliceFrames]
        val encoderInput = FloatArray(N_MELS * sliceFrames)
        for (m in 0 until N_MELS) {
            for (t in 0 until sliceFrames) {
                encoderInput[m * sliceFrames + t] = features.data[m * totalFrames + (startFrame + t)]
            }
        }

        // Run encoder
        t0 = System.nanoTime()
        val encoderOut = runEncoder(encoderInput, sliceFrames)
        val encMs = (System.nanoTime() - t0) / 1_000_000
        totalEncMs += encMs

        val encFrames = encoderOut.size / ENC_DIM
        if (encFrames == 0) return ""

        // Decode each output frame
        t0 = System.nanoTime()
        val tokens = mutableListOf<Int>()

        for (t in 0 until encFrames) {
            // Extract single frame: [1, 512, 1]
            val frame = FloatArray(ENC_DIM)
            for (d in 0 until ENC_DIM) {
                frame[d] = encoderOut[d * encFrames + t]
            }

            var symsAdded = 0
            while (symsAdded < 5) {
                val result = runDecoder(frame)

                val maxIdx = argmax(result.logits)

                if (maxIdx == tokenizer.blankId || maxIdx == 0) break

                if (maxIdx == tokenizer.eouId) {
                    resetDecoderStates()
                    break
                }

                if (maxIdx >= tokenizer.vocabSize) break

                stateH = result.newH
                stateC = result.newC
                lastToken = maxIdx
                tokens.add(maxIdx)
                symsAdded++
            }
        }

        val decMs = (System.nanoTime() - t0) / 1_000_000
        totalDecMs += decMs

        // Log timing every 10 chunks (~1.6s of audio)
        if (chunkCount % 10 == 0L) {
            Log.d(TAG, "chunk=$chunkCount avg mel=${totalMelMs/chunkCount}ms enc=${totalEncMs/chunkCount}ms dec=${totalDecMs/chunkCount}ms total=${(totalMelMs+totalEncMs+totalDecMs)/chunkCount}ms (budget=160ms)")
        }

        return if (tokens.isEmpty()) "" else tokenizer.decode(tokens.toIntArray())
    }

    private fun runEncoder(input: FloatArray, timeSteps: Int): FloatArray {
        val audioSignal = OnnxTensor.createTensor(
            env, FloatBuffer.wrap(input), longArrayOf(1, N_MELS.toLong(), timeSteps.toLong()),
        )
        val length = OnnxTensor.createTensor(
            env, LongBuffer.wrap(longArrayOf(timeSteps.toLong())), longArrayOf(1),
        )
        val cacheChTensor = OnnxTensor.createTensor(
            env, FloatBuffer.wrap(cacheChannel), longArrayOf(17, 1, 70, 512),
        )
        val cacheTimeTensor = OnnxTensor.createTensor(
            env, FloatBuffer.wrap(cacheTime), longArrayOf(17, 1, 512, 8),
        )
        val cacheLenTensor = OnnxTensor.createTensor(
            env, LongBuffer.wrap(cacheLen), longArrayOf(1),
        )

        val inputs = mapOf(
            "audio_signal" to audioSignal,
            "length" to length,
            "cache_last_channel" to cacheChTensor,
            "cache_last_time" to cacheTimeTensor,
            "cache_last_channel_len" to cacheLenTensor,
        )

        val outputs = encoderSession.run(inputs)

        // Extract encoder output [1, 512, T]
        val outTensor = outputs["outputs"].get() as OnnxTensor
        val outData = outTensor.floatBuffer
        val result = FloatArray(outData.remaining())
        outData.get(result)

        // Update cache
        val newChTensor = outputs["new_cache_last_channel"].get() as OnnxTensor
        val newChData = newChTensor.floatBuffer
        cacheChannel = FloatArray(newChData.remaining()).also { newChData.get(it) }

        val newTimeTensor = outputs["new_cache_last_time"].get() as OnnxTensor
        val newTimeData = newTimeTensor.floatBuffer
        cacheTime = FloatArray(newTimeData.remaining()).also { newTimeData.get(it) }

        val newLenTensor = outputs["new_cache_last_channel_len"].get() as OnnxTensor
        val newLenData = newLenTensor.longBuffer
        cacheLen = LongArray(newLenData.remaining()).also { newLenData.get(it) }

        outputs.close()
        audioSignal.close()
        length.close()
        cacheChTensor.close()
        cacheTimeTensor.close()
        cacheLenTensor.close()

        return result
    }

    private data class DecoderResult(
        val logits: FloatArray,
        val newH: FloatArray,
        val newC: FloatArray,
    )

    private fun runDecoder(encoderFrame: FloatArray): DecoderResult {
        val encTensor = OnnxTensor.createTensor(
            env, FloatBuffer.wrap(encoderFrame), longArrayOf(1, ENC_DIM.toLong(), 1),
        )
        val targetTensor = OnnxTensor.createTensor(
            env, IntBuffer.wrap(intArrayOf(lastToken)), longArrayOf(1, 1),
        )
        val targetLenTensor = OnnxTensor.createTensor(
            env, IntBuffer.wrap(intArrayOf(1)), longArrayOf(1),
        )
        val hTensor = OnnxTensor.createTensor(
            env, FloatBuffer.wrap(stateH), longArrayOf(1, 1, 640),
        )
        val cTensor = OnnxTensor.createTensor(
            env, FloatBuffer.wrap(stateC), longArrayOf(1, 1, 640),
        )

        val inputs = mapOf(
            "encoder_outputs" to encTensor,
            "targets" to targetTensor,
            "target_length" to targetLenTensor,
            "input_states_1" to hTensor,
            "input_states_2" to cTensor,
        )

        val outputs = decoderSession.run(inputs)

        val logitsTensor = outputs["outputs"].get() as OnnxTensor
        val logitsBuf = logitsTensor.floatBuffer
        val logits = FloatArray(logitsBuf.remaining()).also { logitsBuf.get(it) }

        val newHTensor = outputs["output_states_1"].get() as OnnxTensor
        val newHBuf = newHTensor.floatBuffer
        val newH = FloatArray(newHBuf.remaining()).also { newHBuf.get(it) }

        val newCTensor = outputs["output_states_2"].get() as OnnxTensor
        val newCBuf = newCTensor.floatBuffer
        val newC = FloatArray(newCBuf.remaining()).also { newCBuf.get(it) }

        outputs.close()
        encTensor.close()
        targetTensor.close()
        targetLenTensor.close()
        hTensor.close()
        cTensor.close()

        return DecoderResult(logits, newH, newC)
    }

    private fun argmax(arr: FloatArray): Int {
        var maxIdx = 0
        var maxVal = Float.NEGATIVE_INFINITY
        for (i in arr.indices) {
            if (arr[i].isFinite() && arr[i] > maxVal) {
                maxVal = arr[i]
                maxIdx = i
            }
        }
        return maxIdx
    }

    private fun resetDecoderStates() {
        stateH.fill(0f)
        stateC.fill(0f)
        lastToken = tokenizer.blankId
    }

    fun reset() {
        resetDecoderStates()
        cacheChannel.fill(0f)
        cacheTime.fill(0f)
        cacheLen[0] = 0L
        audioBuffer.clear()
    }

    fun close() {
        encoderSession.close()
        decoderSession.close()
    }

    companion object {
        private const val TAG = "ParakeetEngine"
        private const val SAMPLE_RATE = 16000
        private const val BUFFER_SIZE = SAMPLE_RATE * 4 // 4 seconds
        private const val N_MELS = MelSpectrogram.N_MELS
        private const val ENC_DIM = 512
        private const val PRE_ENCODE_CACHE = 9
        private const val FRAMES_PER_CHUNK = 16
        private const val SLICE_LEN = PRE_ENCODE_CACHE + FRAMES_PER_CHUNK
    }
}

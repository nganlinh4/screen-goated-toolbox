package dev.screengoated.toolbox.mobile.preset

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import java.io.ByteArrayOutputStream
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class GeminiLiveStillFrameDeviceTest {
    @Test
    fun smallTextSelectionKeepsItsPixelsThroughTheActualEncoder() {
        val input = smallSelectionPng()
        val encoder = Class.forName(
            "dev.screengoated.toolbox.mobile.preset.GeminiLivePresetClientKt",
        ).getDeclaredMethod("buildGeminiLiveStillFrame", ByteArray::class.java, String::class.java)
        encoder.isAccessible = true
        val frame = encoder.invoke(null, input, "image/png") as Pair<*, *>
        assertEquals("image/jpeg", frame.second)
        val bytes = frame.first as ByteArray
        InstrumentationRegistry.getInstrumentation().targetContext.cacheDir
            .resolve("live-still-frame-output.jpg").writeBytes(bytes)
        val decoded = requireNotNull(BitmapFactory.decodeByteArray(bytes, 0, bytes.size))
        try {
            assertEquals("small selection width", 320, decoded.width)
            assertEquals("small selection height", 60, decoded.height)
        } finally {
            decoded.recycle()
        }
    }

    @Test
    fun savedKeyLiveProviderReadsTheSmallSelection() = runBlocking {
        val arguments = InstrumentationRegistry.getArguments()
        assumeTrue(arguments.getString("sgtLiveProvider") == "true")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val container = (context.applicationContext as SgtMobileApplication).appContainer
        val key = container.repository.currentApiKey()
        assertTrue("Save an existing Gemini key in the app first", key.isNotBlank())
        val models = PresetModelCatalog.forType(PresetModelType.VISION)
            .filter { it.provider == PresetModelProvider.GEMINI_LIVE }
        val requested = arguments.getString("sgtLiveVisionModelId")
        val model = requireNotNull(if (requested == null) models.firstOrNull() else models.find { it.id == requested })
        val began = System.nanoTime()
        val result = withTimeout(90_000) {
            VisionApiClient(container.httpClient).executeStreaming(
                modelId = model.id, prompt = "Read all text in the image. Return only the text.",
                imageBytes = smallSelectionPng(), apiKeys = ApiKeys(geminiKey = key),
                uiLanguage = "en", onChunk = {},
            )
        }
        // Provider exceptions can contain request URLs; do not print them or credentials.
        val answer = result.getOrNull()
        assertNotNull("Live image request did not complete successfully", answer)
        context.cacheDir.resolve("live-still-frame-provider.txt").writeText(
            "model=${model.id}\nelapsed_ms=${(System.nanoTime() - began) / 1_000_000}\nanswer=$answer\n",
        )
        assertEquals("frame320by60", requireNotNull(answer).lowercase().filter(Char::isLetterOrDigit))
    }

    private fun smallSelectionPng(): ByteArray {
        val source = Bitmap.createBitmap(320, 60, Bitmap.Config.ARGB_8888)
        Canvas(source).apply {
            drawColor(Color.WHITE)
            drawText("Frame 320 by 60", 8f, 38f, Paint().apply {
                color = Color.BLACK
                textSize = 24f
                isAntiAlias = true
            })
        }
        val input = ByteArrayOutputStream().use {
            check(source.compress(Bitmap.CompressFormat.PNG, 100, it))
            it.toByteArray()
        }
        source.recycle()
        return input
    }
}

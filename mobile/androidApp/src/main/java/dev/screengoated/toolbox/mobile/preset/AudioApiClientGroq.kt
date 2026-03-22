package dev.screengoated.toolbox.mobile.preset

import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.Request
import okhttp3.RequestBody.Companion.asRequestBody
import java.io.IOException

internal fun AudioApiClient.transcribeWithGroq(
    model: PresetModelDescriptor,
    wavBytes: ByteArray,
    apiKey: String,
): String {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:groq")

    val tempFile = kotlin.io.path.createTempFile(prefix = "sgt_audio_", suffix = ".wav").toFile()
    tempFile.writeBytes(wavBytes)
    try {
        val multipartBody = MultipartBody.Builder()
            .setType(MultipartBody.FORM)
            .addFormDataPart("model", model.fullName)
            .addFormDataPart(
                "file",
                "audio.wav",
                tempFile.asRequestBody("audio/wav".toMediaType()),
            )
            .build()

        val request = Request.Builder()
            .url("https://api.groq.com/openai/v1/audio/transcriptions")
            .header("Authorization", "Bearer $apiKey")
            .post(multipartBody)
            .build()

        httpClient.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                val code = response.code
                if (code == 401 || code == 403) throw IOException("INVALID_API_KEY")
                throw IOException("Groq audio request failed with $code")
            }
            val body = response.body?.string().orEmpty()
            return org.json.JSONObject(body).optString("text")
        }
    } finally {
        tempFile.delete()
    }
}

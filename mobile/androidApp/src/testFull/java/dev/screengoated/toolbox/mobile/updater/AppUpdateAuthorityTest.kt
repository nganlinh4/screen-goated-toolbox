package dev.screengoated.toolbox.mobile.updater

import java.io.File
import java.io.IOException
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody
import okhttp3.MediaType
import okio.Buffer
import okio.BufferedSource
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class AppUpdateAuthorityTest {
    private fun fixture(): JSONObject = JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
        .map { File(it,"parity-fixtures/app-update/authority.json") }.first(File::isFile).readText())

    @Test
    fun signaturesBindStrictlyEncodedManifestBytes() {
        val fixture=fixture()
        val key=decode(fixture.getString("public_key_hex"))
        val vectors=fixture.getJSONObject("vectors")
        vectors.keys().forEach { name ->
            val vector=vectors.getJSONObject(name)
            val result=runCatching { verifiedStableManifest(decode(vector.getString("payload_hex")),decode(vector.getString("signature_hex")),key) }
            assertEquals(name,vector.getBoolean("accepted"),result.isSuccess)
        }
    }

    @Test
    fun onlyMissingPrimaryAllowsFallbackThroughHttpAndVerification() {
        val fixture=fixture()
        val cases=fixture.getJSONArray("http_cases")
        val release=JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .map { File(it,"parity-fixtures/app-update/latest-release.json") }.first(File::isFile).readText()).getJSONObject("latest_release")
        for (index in 0 until cases.length()) {
            val case=cases.getJSONObject(index)
            val requests=mutableListOf<String>()
            val vector=fixture.getJSONObject("vectors").getJSONObject(case.getString("vector"))
            val client=OkHttpClient.Builder().addInterceptor { chain ->
                val request=chain.request()
                val resource=when {
                    request.url.encodedPath.endsWith("stable-v1.json") -> "manifest"
                    request.url.encodedPath.endsWith("stable-v1.sig") -> "signature"
                    else -> "fallback"
                }
                requests+=resource
                val status=if(resource=="fallback") 200 else case.getInt("${resource}_status")
                if(status==0) throw IOException("Owned transport failure")
                var payload=if(resource=="fallback") JSONArray().put(release).toString().encodeToByteArray()
                    else decode(vector.getString(if(resource=="manifest") "payload_hex" else "signature_hex"))
                if(resource=="fallback" && case.optString("fallback_mode")=="invalid_utf8") {
                    val text=JSONArray().put(JSONObject(release.toString()).put("body","UTF8_SENTINEL")).toString()
                    payload=text.encodeToByteArray()
                    val marker="UTF8_SENTINEL".encodeToByteArray()
                    val offset=(0..payload.size-marker.size).first { start ->
                        marker.indices.all { payload[start+it]==marker[it] }
                    }
                    payload[offset]=0xff.toByte()
                }
                val mode=case.optString("body_mode")
                if(resource=="manifest" && mode.startsWith("manifest_")) payload=ByteArray(MAXIMUM_MANIFEST_BYTES+1)
                if(resource=="signature" && mode=="signature_oversize") payload+=0.toByte()
                if(resource=="signature" && mode=="short_signature") payload=payload.copyOf(63)
                val body=object : ResponseBody() {
                    private val buffer=Buffer().write(payload)
                    override fun contentType(): MediaType? = null
                    override fun contentLength(): Long = if(mode=="manifest_oversize" || mode=="signature_oversize") -1 else payload.size.toLong()
                    override fun source(): BufferedSource = buffer
                }
                Response.Builder().request(request).protocol(Protocol.HTTP_1_1).code(status).message("Owned response").body(body).build()
            }.build()
            val result=AppUpdateSource(client) {
                when(case.optString("key_mode")) {
                    "public_key_unavailable" -> throw IOException("Owned key read failure")
                    "public_key_invalid" -> ByteArray(65)
                    else -> decode(fixture.getString("public_key_hex"))
                }
            }.fetchLatestRelease()
            val expected=case.getString("expected")
            assertEquals(case.getString("name"),expected!="error",result.isSuccess)
            val paths=case.getJSONArray("requests")
            assertEquals(case.getString("name"),(0 until paths.length()).map(paths::getString),requests)
        }
    }

    private fun decode(value: String): ByteArray = ByteArray(value.length / 2) { index ->
        value.substring(index*2,index*2+2).toInt(16).toByte()
    }
}

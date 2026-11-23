package dev.screengoated.toolbox.mobile.updater

import java.io.File
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Test

class AppUpdateFallbackSelectionTest {
    private fun fixture(name: String): JSONObject = JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
        .map { File(it,"parity-fixtures/app-update/$name.json") }.first(File::isFile).readText())

    @Test
    fun sharedFallbackSelectionRequiresACompleteUnambiguousHighestRelease() {
        val fixture=fixture("fallback-selection")
        val base=fixture("latest-release").getJSONObject("latest_release")
        val cases=fixture.getJSONArray("cases")
        val failures=mutableListOf<String>()
        for(index in 0 until cases.length()) {
            val case=cases.getJSONObject(index)
            val pages=buildList {
                val encoded=case.getJSONArray("pages")
                for(page in 0 until encoded.length()) {
                    val value=encoded.getJSONObject(page)
                    repeat(value.optInt("repeat",1)) { add(value) }
                }
            }
            val requests=mutableListOf<Int>()
            var primaryRequests=0
            val client=OkHttpClient.Builder().addInterceptor { chain ->
                val request=chain.request()
                val status: Int
                val body: String
                if(request.url.encodedPath.endsWith("stable-v1.json")) {
                    primaryRequests++
                    status=404
                    body=""
                } else {
                    require(request.url.encodedPath.endsWith("/releases"))
                    require(request.url.queryParameter("per_page")==fixture.getJSONObject("limits").getInt("page_size").toString())
                    val page=requireNotNull(request.url.queryParameter("page")).toInt()
                    requests+=page
                    val value=pages.getOrNull(page-1) ?: error("Unexpected release page")
                    status=value.optInt("http_status",200)
                    val releases=JSONArray()
                    val entries=value.optJSONArray("entries") ?: JSONArray()
                    for(entry in 0 until entries.length()) {
                        val item=entries.getJSONObject(entry)
                        repeat(item.getInt("count")) { releases.put(release(fixture,base,item.getString("release"))) }
                    }
                    body=releases.toString()
                }
                Response.Builder().request(request).protocol(Protocol.HTTP_1_1).code(status).message("Owned page").body(body.toResponseBody()).build()
            }.build()
            val result=AppUpdateSource(client) { error("Missing primary must not load a key") }.fetchLatestRelease()
            val expected=case.getBoolean("android_full")
            val selected=if(expected) release(fixture,base,case.getString("selected")) else null
            val candidate=result.getOrNull()
            val acceptedMatches=result.isSuccess==expected
            val selectedMatches=selected==null || candidate?.let {
                val asset=selected.getJSONArray("assets").getJSONObject(1)
                it.version==selected.getString("tag_name").removePrefix("v") && it.body==selected.getString("body") &&
                    it.assetName==asset.getString("name") && it.assetUrl==asset.getString("browser_download_url") &&
                    it.sizeBytes==asset.getLong("size") && it.sha256==asset.getString("digest").removePrefix("sha256:")
            }==true
            if(!acceptedMatches || !selectedMatches || primaryRequests!=1 || requests!=(1..case.getInt("requests")).toList()) {
                failures+="${case.getString("name")}:accepted=${result.isSuccess},pages=$requests,candidate=$candidate"
            }
        }
        assertTrue(failures.joinToString("\n"),failures.isEmpty())
    }

    private fun release(fixture: JSONObject,base: JSONObject,name: String): JSONObject {
        val descriptor=fixture.getJSONObject("releases").getJSONObject(name)
        val defaults=fixture.getJSONObject("defaults")
        val version=descriptor.getString("version")
        return JSONObject(base.toString()).apply {
            put("tag_name","v$version")
            put("body",descriptor.getString("body"))
            put("draft",descriptor.optBoolean("draft",false))
            val assets=getJSONArray("assets")
            for(index in 0 until assets.length()) {
                val asset=assets.getJSONObject(index)
                val extension=if(asset.getString("name").endsWith(".exe")) "exe" else "apk"
                val platform=if(extension=="exe") "windows" else "apk"
                val filename="ScreenGoatedToolbox_v$version.$extension"
                asset.put("name",filename)
                asset.put("browser_download_url","https://github.com/nganlinh4/screen-goated-toolbox/releases/download/v$version/$filename")
                asset.put("size",descriptor.optLong("${platform}_size",defaults.getLong("${platform}_size")))
                asset.put("digest","sha256:"+descriptor.optString("${platform}_sha256",defaults.getString("${platform}_sha256")))
            }
        }
    }
}

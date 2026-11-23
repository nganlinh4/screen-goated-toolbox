package dev.screengoated.toolbox.mobile.updater

import java.io.File
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class AppUpdateContractParityTest {
    private fun fixture(): JSONObject = JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
        .map { File(it,"parity-fixtures/app-update/latest-release.json") }.first(File::isFile).readText())

    @Test
    fun sharedManifestAcceptanceCasesMatchTheCompleteContract() = replay("manifest")

    @Test
    fun sharedFallbackAcceptanceCasesMatchThePlatformPackage() = replay("fallback")

    private fun replay(source: String) {
        val fixture=fixture()
        val cases=fixture.getJSONArray("acceptance_cases")
        val failures=mutableListOf<String>()
        for (index in 0 until cases.length()) {
            val case=cases.getJSONObject(index)
            if (case.getString("source")!=source) continue
            val payload=JSONObject(fixture.getJSONObject(if(source=="manifest") "stable_manifest" else "latest_release").toString())
            val mutations=case.getJSONArray("mutations")
            for (step in 0 until mutations.length()) mutate(payload,mutations.getJSONObject(step))
            val result=runCatching {
                if(source=="manifest") stableManifestCandidate(payload.toString().encodeToByteArray())
                else githubCandidates(JSONArray().put(payload).toString().encodeToByteArray()).single()
            }
            if (result.isSuccess!=case.getBoolean("android_full")) failures+=case.getString("name")
            if (result.isSuccess && case.getBoolean("android_full")) {
                val candidate=result.getOrThrow()
                assertEquals("ScreenGoatedToolbox_v9.9.9.apk",candidate.assetName)
                assertEquals(98_765_432L,candidate.sizeBytes)
                assertEquals("2".repeat(64),candidate.sha256)
                assertEquals(case.optString("body","- Synthetic release note"),candidate.body)
            }
        }
        assertEquals("Unexpected acceptance for $source: $failures",emptyList<String>(),failures)
    }

    private fun mutate(root: JSONObject, mutation: JSONObject) {
        val parts=mutation.getString("pointer").removePrefix("/").split('/')
        var parent: Any=root
        for (part in parts.dropLast(1)) parent=when(parent) {
            is JSONObject -> parent.get(part)
            is JSONArray -> parent.get(part.toInt())
            else -> error("Invalid fixture pointer")
        }
        val field=parts.last()
        when(mutation.optString("operation","replace")) {
            "remove" -> (parent as JSONObject).remove(field)
            "duplicate" -> {
                val array=parent as JSONArray
                array.put(JSONObject(array.getJSONObject(field.toInt()).toString()))
            }
            "replace" -> when(parent) {
                is JSONObject -> parent.put(field,mutation.get("value"))
                is JSONArray -> parent.put(field.toInt(),mutation.get("value"))
                else -> error("Invalid fixture target")
            }
            else -> error("Invalid fixture operation")
        }
    }
}

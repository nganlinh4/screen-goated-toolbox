package dev.screengoated.toolbox.mobile.preset

import java.io.File
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class AdaptiveMergeParityTest {
    private fun fixture(): JSONObject = JSONObject(
        generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .map { File(it, "parity-fixtures/preset-system/adaptive-merge.json") }
            .first(File::isFile)
            .readText(),
    )

    private fun JSONArray.strings(): List<String> = (0 until length()).map(::getString)

    @Test
    fun sharedAdaptiveMergeCasesMatchWindows() {
        val cases = fixture().getJSONArray("cases")
        for (index in 0 until cases.length()) {
            val case = cases.getJSONObject(index)
            val ranks = case.getJSONObject("ranks")
            val merged = mergeAdaptiveModels(
                configured = case.getJSONArray("chain").strings(),
                offered = case.getJSONArray("offered").strings(),
                pinned = case.getJSONArray("pinned").strings(),
                excluded = case.getJSONArray("excluded").strings(),
                protectedLocalLeaders = case.getInt("protectedLocalLeaders"),
            ) { id ->
                val rank = ranks.getJSONArray(id)
                AdaptiveCandidateRank(qualityTier = rank.getInt(0), latencyMs = rank.getInt(1))
            }
            assertEquals(case.getString("name"), case.getJSONArray("expected").strings(), merged)
        }
    }
}

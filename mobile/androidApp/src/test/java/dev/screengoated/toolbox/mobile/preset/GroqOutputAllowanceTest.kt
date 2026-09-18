package dev.screengoated.toolbox.mobile.preset

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.File

class GroqOutputAllowanceTest {
    @Test
    fun sharedOutputAllowanceRetryContract() {
        val cases = JSONObject(File("../../parity-fixtures/preset-system/groq-output-allowance.json").readText()).getJSONArray("cases")
        for (index in 0 until cases.length()) {
            val case = cases.getJSONObject(index)
            assertEquals(case.getString("name"), case.getBoolean("request_scoped"), groqRequestLimitPrefix(case.getInt("status"), case.getJSONObject("body").toString()).isNotEmpty())
            val result = groqOutputAllowanceRetry(case.getInt("status"), case.getInt("attempt") != 0,
                case.getJSONObject("body").toString(), if (case.isNull("current")) null else case.getLong("current"))
            assertEquals(case.getString("name"), if (case.isNull("expected")) null else case.getInt("expected"), result)
        }
    }
}

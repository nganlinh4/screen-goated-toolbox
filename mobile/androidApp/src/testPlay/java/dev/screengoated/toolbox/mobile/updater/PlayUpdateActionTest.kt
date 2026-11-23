package dev.screengoated.toolbox.mobile.updater

import java.io.File
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class PlayUpdateActionTest {
    @Test
    fun sharedPlayLaunchOutcomesReportActualAcceptance() {
        val fixture=JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .map { File(it,"parity-fixtures/app-update/play-flow.json") }.first(File::isFile).readText())
        val cases=fixture.getJSONArray("cases")
        for (index in 0 until cases.length()) {
            val case=cases.getJSONObject(index)
            var calls=0
            val accepted=tryStartPlayUpdate {
                calls++
                if (case.getBoolean("throws")) throw IllegalStateException("unavailable")
                case.getBoolean("returned")
            }
            assertEquals(case.getString("name"),case.getBoolean("accepted"),accepted)
            assertEquals(1,calls)
        }
    }
}

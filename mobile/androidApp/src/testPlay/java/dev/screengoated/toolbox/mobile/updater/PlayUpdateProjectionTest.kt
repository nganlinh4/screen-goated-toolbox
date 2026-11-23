package dev.screengoated.toolbox.mobile.updater

import java.io.File
import com.google.android.play.core.install.model.InstallStatus
import com.google.android.play.core.install.model.UpdateAvailability
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class PlayUpdateProjectionTest {
    @Test fun sharedQuerySnapshotsNeverPretendAnUnknownUpdateIsCurrent() {
        val fixture=JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .map { File(it,"parity-fixtures/app-update/play-query.json") }.first(File::isFile).readText())
        val cases=fixture.getJSONArray("cases");val failures=mutableListOf<String>()
        val installs=mapOf("unknown" to InstallStatus.UNKNOWN,"pending" to InstallStatus.PENDING,
            "downloading" to InstallStatus.DOWNLOADING,"downloaded" to InstallStatus.DOWNLOADED,"installing" to InstallStatus.INSTALLING)
        val availability=mapOf("unknown" to UpdateAvailability.UNKNOWN,"not_available" to UpdateAvailability.UPDATE_NOT_AVAILABLE,
            "available" to UpdateAvailability.UPDATE_AVAILABLE,"in_progress" to UpdateAvailability.DEVELOPER_TRIGGERED_UPDATE_IN_PROGRESS)
        for (index in 0 until cases.length()) {
            val case=cases.getJSONObject(index)
            var flexibleReads=0;var versionReads=0
            val result=playUpdateProjection(installs.getValue(case.getString("install")),availability.getValue(case.getString("availability")),
                { flexibleReads++;case.getBoolean("flexible") },{ versionReads++;case.getInt("versionCode") },"1.0.0")
            if(result.status.name.lowercase()!=case.getString("status")) failures+=case.getString("name")
            assertNull(result.notificationVersion)
            if(case.getString("install")!="unknown") {assertEquals(0,flexibleReads);assertEquals(0,versionReads)}
        }
        assertEquals("Wrong query projections: $failures",emptyList<String>(),failures)
    }

    @Test fun versionCode50700_displays5_7_0() {
        val result=playUpdateProjection(InstallStatus.UNKNOWN,UpdateAvailability.UPDATE_AVAILABLE,{ true },{ 50700 },"5.6.0")
        assertEquals(AppUpdateStatus.UPDATE_AVAILABLE,result.status)
        assertEquals("5.7.0",result.latestVersion)
        assertEquals("4.10.1",playVersionName(41001))
    }
}

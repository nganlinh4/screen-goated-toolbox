package dev.screengoated.toolbox.mobile.updater

import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import org.json.JSONObject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest

class AppUpdateStateOwnerTest {
    private fun available()=AppUpdateProjection(AppUpdateStatus.UPDATE_AVAILABLE,latestVersion="9.9.9",notificationVersion="9.9.9")

    @Test fun aLateCheckCannotReplaceADownloadedUpdate() {
        val owner=AppUpdateStateOwner<String>("1.0.0")
        val check=requireNotNull(owner.beginCheck());owner.installState(AppUpdateStatus.DOWNLOADED)
        owner.finishCheck(check,available(),"old")
        assertEquals(AppUpdateStatus.DOWNLOADED,owner.state.value.status)
    }
    @Test fun checksCannotReplaceAReadyInstall() {
        val owner=AppUpdateStateOwner<String>("1.0.0")
        owner.installState(AppUpdateStatus.DOWNLOADED)
        assertNull(owner.beginCheck())
    }
    @Test fun aFreshCheckRetiresThePreviousAction() {
        val owner=AppUpdateStateOwner<String>("1.0.0")
        val check=requireNotNull(owner.beginCheck());owner.finishCheck(check,available(),"old");owner.beginCheck()
        assertNull(owner.claimAction())
    }
    @Test fun aCheckedActionCanBeClaimedOnlyOnce() {
        val owner=AppUpdateStateOwner<String>("1.0.0")
        val check=requireNotNull(owner.beginCheck());owner.finishCheck(check,available(),"action")
        assertEquals("action",owner.claimAction()?.value);assertNull(owner.claimAction())
    }
    @Test fun anOldProgressEventCannotRegressDownloadedState() {
        val owner=AppUpdateStateOwner<String>("1.0.0")
        owner.installState(AppUpdateStatus.DOWNLOADED);owner.installState(AppUpdateStatus.DOWNLOADING)
        assertEquals(AppUpdateStatus.DOWNLOADED,owner.state.value.status)
    }
    @Test fun anAvailableNotificationCanBeConsumedOnlyOnce() {
        val owner=AppUpdateStateOwner<String>("1.0.0")
        val check=requireNotNull(owner.beginCheck());owner.finishCheck(check,available())
        val notice=owner.state.value.notification
        assertEquals("9.9.9",owner.consumeNotification(notice));assertNull(owner.consumeNotification(notice))
    }
    @Test fun startupChecksRunOnce() {
        val owner=AppUpdateStateOwner<String>("1.0.0")
        val check=requireNotNull(owner.beginAutoCheck());owner.finishCheck(check,AppUpdateProjection(AppUpdateStatus.UP_TO_DATE))
        assertNull(owner.beginAutoCheck())
    }

    @Test fun sharedFullLifecycleTimelinesMatchTheOwner() = replay("full")
    @Test fun sharedPlayLifecycleTimelinesMatchTheOwner() = replay("play")

    private fun replay(platform: String) {
        val fixture=JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .map { File(it,"parity-fixtures/app-update/lifecycle.json") }.first(File::isFile).readText())
        val cases=fixture.getJSONArray("cases")
        for (index in 0 until cases.length()) {
            val case=cases.getJSONObject(index)
            val platforms=case.getJSONArray("platforms")
            if ((0 until platforms.length()).none { platforms.getString(it)==platform }) continue
            val owner=AppUpdateStateOwner<String>("1.0.0")
            val checks=mutableMapOf<String,AppUpdateCheck>()
            val completions=mutableMapOf<String,AppUpdateCompletion>()
            var notice: AppUpdateNotice?=null
            val steps=case.getJSONArray("steps")
            for (stepIndex in 0 until steps.length()) {
                val step=steps.getJSONObject(stepIndex);val id=step.optString("id")
                when(step.getString("action")) {
                    "check","auto" -> {
                        val check=if(step.getString("action")=="auto") owner.beginAutoCheck() else owner.beginCheck()
                        assertEquals(case.getString("name"),step.getBoolean("accepted"),check!=null)
                        if(check!=null) checks[id]=check
                    }
                    "available" -> {
                        owner.finishCheck(checks.getValue(id),available().copy(notificationVersion="9.9.9".takeIf { platform=="full" }),"action")
                        owner.state.value.notification?.let { notice=it }
                    }
                    "up_to_date" -> owner.finishCheck(checks.getValue(id),AppUpdateProjection(AppUpdateStatus.UP_TO_DATE))
                    "error" -> owner.finishCheck(checks.getValue(id),AppUpdateProjection(AppUpdateStatus.ERROR,errorMessage="unavailable"))
                    "notice" -> assertEquals(case.getString("name"),step.getBoolean("accepted"),owner.consumeNotification(notice)!=null)
                    "start" -> {
                        val launch=owner.claimAction()
                        assertEquals(case.getString("name"),step.getBoolean("accepted"),launch!=null)
                        if(launch!=null) owner.finishLaunch(launch,step.optBoolean("returned",true),"unavailable")
                    }
                    "ready" -> owner.installState(AppUpdateStatus.DOWNLOADED)
                    "progress" -> owner.installState(AppUpdateStatus.DOWNLOADING)
                    "complete" -> {
                        val completion=owner.beginCompletion()
                        assertEquals(case.getString("name"),step.getBoolean("accepted"),completion!=null)
                        if(completion!=null) completions[id]=completion
                    }
                    "completion_error" -> owner.finishCompletion(completions.getValue(id),"unavailable")
                    else -> error("Unknown lifecycle fixture action")
                }
            }
            val status=when(owner.state.value.status) {
                AppUpdateStatus.IDLE->"idle";AppUpdateStatus.CHECKING->"checking"
                AppUpdateStatus.UPDATE_AVAILABLE->"available";AppUpdateStatus.UP_TO_DATE->"up_to_date"
                AppUpdateStatus.DOWNLOADING->"downloading";AppUpdateStatus.DOWNLOADED->"ready"
                AppUpdateStatus.INSTALLING->"installing";AppUpdateStatus.ERROR->"error"
            }
            assertEquals(case.getString("name"),case.getString("status"),status)
        }
    }

    @Test fun completionFailureKeepsTheDownloadedUpdateRetryable() {
        val owner=AppUpdateStateOwner<String>("1.0.0");owner.installState(AppUpdateStatus.DOWNLOADED)
        val completion=requireNotNull(owner.beginCompletion())
        assertEquals(AppUpdateStatus.INSTALLING,owner.state.value.status)
        assertNull(owner.beginCheck());assertNull(owner.beginCompletion())
        owner.finishCompletion(completion,"unavailable")
        assertEquals(AppUpdateStatus.DOWNLOADED,owner.state.value.status);assertEquals("unavailable",owner.state.value.errorMessage)
        assertNotNull(owner.beginCompletion())
    }

    @Test fun aRetiredCompletionCannotUndoANewerRequest() {
        val owner=AppUpdateStateOwner<String>("1.0.0");owner.installState(AppUpdateStatus.DOWNLOADED)
        val old=requireNotNull(owner.beginCompletion());owner.finishCompletion(old,"unavailable")
        val current=requireNotNull(owner.beginCompletion());owner.finishCompletion(old,"old failure")
        assertEquals(AppUpdateStatus.INSTALLING,owner.state.value.status)
        owner.finishCompletion(current,null);owner.finishCompletion(current,"late failure")
        assertEquals(AppUpdateStatus.INSTALLING,owner.state.value.status)
    }

    @Test fun equalVersionNoticesStillHaveIndependentOwnership() {
        val owner=AppUpdateStateOwner<String>("1.0.0")
        owner.finishCheck(requireNotNull(owner.beginCheck()),available());val old=owner.state.value.notification
        owner.finishCheck(requireNotNull(owner.beginCheck()),available());val current=owner.state.value.notification
        assertNotSame(old,current);assertNull(owner.consumeNotification(old));assertEquals("9.9.9",owner.consumeNotification(current))
    }

    @Test fun concurrentChecksHaveOneAdmittedOwner() {
        val owner=AppUpdateStateOwner<String>("1.0.0");val executor=Executors.newFixedThreadPool(8);val start=CountDownLatch(1)
        try {
            val futures=(0 until 8).map { executor.submit<Boolean> {start.await();owner.beginCheck()!=null} }
            start.countDown();assertEquals(1,futures.count { it.get(5,TimeUnit.SECONDS) })
        } finally {executor.shutdownNow()}
    }

    @Test fun aManualCheckAlsoCoalescesTheOneStartupCheck() {
        val owner=AppUpdateStateOwner<String>("1.0.0");val check=requireNotNull(owner.beginCheck())
        assertNull(owner.beginAutoCheck());owner.finishCheck(check,AppUpdateProjection(AppUpdateStatus.ERROR))
        assertNull(owner.beginAutoCheck());assertNotNull(owner.beginCheck())
    }

    @Test fun aNativeDownloadEventWinsOverTheLaterLaunchReturn() {
        val owner=AppUpdateStateOwner<String>("1.0.0")
        owner.finishCheck(requireNotNull(owner.beginCheck()),available(),"action")
        val launch=requireNotNull(owner.claimAction());owner.installState(AppUpdateStatus.DOWNLOADED)
        owner.finishLaunch(launch,false,"late failure")
        assertEquals(AppUpdateStatus.DOWNLOADED,owner.state.value.status);assertNull(owner.state.value.errorMessage)
    }

    @Test fun onlyAvailableResultsRetainAnActionOrNotice() {
        val owner=AppUpdateStateOwner<String>("1.0.0")
        owner.finishCheck(requireNotNull(owner.beginCheck()),AppUpdateProjection(AppUpdateStatus.UP_TO_DATE,actionUrl="old",notificationVersion="old"),"old")
        assertNull(owner.claimAction());assertNull(owner.state.value.notification);assertNull(owner.state.value.actionUrl)
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    @Test fun reentrantPublicationCannotReviveARetiredAdmission() = runTest {
        val owner=AppUpdateStateOwner<String>("1.0.0")
        var retiredCheck=false;var retiredCompletion=false
        val observer=backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            owner.state.collect { state ->
                if(state.status==AppUpdateStatus.CHECKING && !retiredCheck) {
                    retiredCheck=true;owner.installState(AppUpdateStatus.DOWNLOADED)
                }
                if(state.status==AppUpdateStatus.INSTALLING && !retiredCompletion) {
                    retiredCompletion=true;owner.installState(AppUpdateStatus.ERROR,"unavailable")
                }
            }
        }
        try {
            assertNull(owner.beginCheck());assertTrue(retiredCheck)
            assertEquals(AppUpdateStatus.DOWNLOADED,owner.state.value.status)
            assertNull(owner.beginCompletion());assertTrue(retiredCompletion)
            assertEquals(AppUpdateStatus.DOWNLOADED,owner.state.value.status)
            assertNotNull(owner.beginCompletion())
        } finally {observer.cancel()}
    }
}

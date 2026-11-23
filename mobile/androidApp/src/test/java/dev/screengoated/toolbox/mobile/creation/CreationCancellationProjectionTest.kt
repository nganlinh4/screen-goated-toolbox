package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CreationCancellationProjectionTest {
    private val prior=CreationNativeItem("published","old","source.png","source.png",stage=CreationNativeStage.DONE,
        status=CreationJobStatus(jobId="published-job",stage="done",progressText="",outputPath="prior.svg"))
    private val running=CreationNativeItem("current","new","source.png","source.png",submitted=true,
        stage=CreationNativeStage.RUNNING,submissionToken="token",status=CreationJobStatus(jobId="job",stage="generating",progressText=""))
    private fun state()=MutableStateFlow(CreationNativeUiState(items=listOf(prior,running),selectedItemId=running.id))

    @Test
    fun sharedCancellationOutcomesStayAuthoritativeAndKeepOtherResults()=runTest {
        val fixture=JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .map { File(it,"parity-fixtures/image-to-svg/input-contract.json") }.first(File::isFile).readText())
        val cases=fixture.getJSONArray("cancellationOutcomes")
        for(index in 0 until cases.length()) {
            val row=cases.getJSONObject(index);val current=state();var changes=0
            val preflight=CreationInputPreflight(this,current,{false},{fail("unexpected submit")},{null},StandardTestDispatcher(testScheduler))
            val result=CreationJobStatus(jobId="job",stage=row.getString("stage"),progressText="",
                outputPath="committed.svg".takeIf { row.getBoolean("keepsPublishedResult") })
            cancelCreationSelection(current,preflight,{id->assertEquals("job",id);listOf(result)}, {throw it},{changes++})
            assertEquals(result,current.value.selectedItem?.status)
            assertEquals(row.getBoolean("terminal"),current.value.selectedItem?.stage?.isTerminal())
            assertEquals(row.getBoolean("canSubmit"),current.value.selectedItem?.isConfigurable())
            assertEquals(!row.getBoolean("terminal"),creationStageIsBusy(result.stage))
            assertEquals(prior,current.value.items.first());assertEquals(1,changes)
        }
    }

    @Test
    fun concurrentCommittedResultCannotBeReplacedByAnOlderCancelReply()=runTest {
        val current=state();val committed=CreationJobStatus(jobId="job",stage="done",progressText="",outputPath="new.svg")
        val preflight=CreationInputPreflight(this,current,{false},{fail("unexpected submit")},{null},StandardTestDispatcher(testScheduler))
        cancelCreationSelection(current,preflight,{
            current.value=current.value.copy(items=listOf(prior,running.copy(stage=CreationNativeStage.DONE,status=committed)))
            listOf(CreationJobStatus(jobId="job",stage="cancelled",progressText=""))
        },{throw it},{})
        assertEquals(committed,current.value.selectedItem?.status)
        assertEquals(CreationNativeStage.DONE,current.value.selectedItem?.stage)
        assertEquals(prior,current.value.items.first())
    }

    @Test
    fun unavailableCancellationDoesNotPretendSuccessOrDropActiveMonitoring()=runTest {
        for (throws in listOf(false,true)) {
            val current=state();var errors=0;var changes=0
            val preflight=CreationInputPreflight(this,current,{false},{fail("unexpected submit")},{null},StandardTestDispatcher(testScheduler))
            cancelCreationSelection(current,preflight,{if(throws) throw IllegalStateException("unavailable") else emptyList()},
                {errors++},{changes++})
            assertEquals(running,current.value.selectedItem)
            assertEquals(1,errors);assertEquals(1,changes)
        }
    }

    @Test
    fun localQueuedCancellationDoesNotNeedOrGuessANativeJob()=runTest {
        val queued=running.copy(stage=CreationNativeStage.QUEUED,status=null)
        val current=MutableStateFlow(CreationNativeUiState(items=listOf(prior,queued),selectedItemId=queued.id))
        val preflight=CreationInputPreflight(this,current,{false},{fail("unexpected submit")},{null},StandardTestDispatcher(testScheduler))
        cancelCreationSelection(current,preflight,{fail("no native job");emptyList()},{throw it},{})
        assertEquals(CreationNativeStage.CANCELLED,current.value.selectedItem?.stage)
        assertEquals(prior,current.value.items.first())
    }

    @Test
    fun cancellationDuringPreflightDoesNotRelabelAPreviousPublishedResult()=runTest {
        val current=MutableStateFlow(CreationNativeUiState(items=listOf(prior),selectedItemId=prior.id))
        val pending=kotlinx.coroutines.CompletableDeferred<String?>();var accepted=0
        val preflight=CreationInputPreflight(this,current,{false},{accepted++},{pending.await()},StandardTestDispatcher(testScheduler))
        preflight.submit();runCurrent()
        cancelCreationSelection(current,preflight,{fail("no native job");emptyList()},{throw it},{})
        pending.complete(null);runCurrent()
        assertEquals(prior,current.value.selectedItem)
        assertEquals(0,accepted)
        assertNull(current.value.validatingItemId)
    }

    @Test
    fun lateActivePollCannotReplaceAnAuthoritativeTerminalResult() {
        for (stage in listOf("cancelled", "done", "failed")) {
            val result=CreationJobStatus(jobId="job",stage=stage,progressText="",
                outputPath="committed.svg".takeIf { stage=="done" })
            val terminal=running.copy(stage=result.toNativeStage(),status=result)
            val current=state().value.copy(items=listOf(prior,terminal))
            val stale=mapOf("job" to running.status!!,"published-job" to prior.status!!.copy(stage="generating"))
            val refreshed=refreshCreationNativeItems(current,stale,null)
            assertEquals(current,refreshed.state)
            assertFalse(refreshed.reachedTerminal)
        }
    }
}

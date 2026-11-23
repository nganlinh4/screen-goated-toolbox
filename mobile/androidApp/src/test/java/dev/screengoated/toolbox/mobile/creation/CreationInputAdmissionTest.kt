package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CreationInputAdmissionTest {
    private val original=CreationNativeItem("original","batch","source.png","source.png",
        stage=CreationNativeStage.DONE,status=CreationJobStatus(jobId="published",stage="done",progressText="",outputPath="prior.svg"))
    private val other=CreationNativeItem("other","other-batch","other.png","other.png")
    private fun state()=MutableStateFlow(CreationNativeUiState(items=listOf(original,other),selectedItemId=original.id))

    @Test
    fun sharedDialogActionsSubmitOnlyAfterSuccessfulValidationAndPreservePublishedResults()=runTest {
        val fixture=JSONObject(generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
            .map { File(it,"parity-fixtures/image-to-svg/input-contract.json") }.first(File::isFile).readText())
        val cases=fixture.getJSONArray("validationActions")
        for (index in 0 until cases.length()) {
            val row=cases.getJSONObject(index);val current=state();var accepted=0;var checks=0;var chooses=0
            val failures=row.getJSONArray("checks")
            val preflight=CreationInputPreflight(this,current,{false},{accepted++},
                { if (failures.isNull(checks++)) null else failures.getString(checks-1) },StandardTestDispatcher(testScheduler))
            preflight.submit();runCurrent()
            val actions=row.getJSONArray("actions")
            for (action in 0 until actions.length()) {
                val notice=requireNotNull(current.value.inputValidation)
                when(actions.getString(action)) {
                    "retry" -> preflight.retry(notice)
                    "choose" -> if(preflight.dismiss(notice)) chooses++
                    "dismiss" -> preflight.dismiss(notice)
                    else -> error("Unsupported fixture action")
                }
                runCurrent()
            }
            assertEquals(row.getBoolean("submits"),accepted==1)
            assertEquals(row.getInt("chooses"),chooses)
            assertEquals(original,current.value.items.first())
            assertEquals("prior.svg",current.value.items.first().status?.outputPath)
            if(actions.getString(0)=="dismiss") assertEquals(fixture.getBoolean("dismissSubmits"),accepted==1)
        }
    }

    @Test
    fun staleDismissCannotEraseANewerEqualNotice()=runTest {
        val current=state()
        val preflight=CreationInputPreflight(this,current,{false},{fail("unexpected submit")},{"image_invalid"},StandardTestDispatcher(testScheduler))
        preflight.submit();runCurrent()
        val old=requireNotNull(current.value.inputValidation)
        assertTrue(preflight.dismiss(old))
        preflight.submit();runCurrent()
        val newer=requireNotNull(current.value.inputValidation)
        assertEquals(old,newer)
        assertNotSame(old,newer)
        assertFalse(preflight.dismiss(old))
        assertSame(newer,current.value.inputValidation)
    }

    @Test
    fun staleRetryCannotSubmitAnotherSelectedSource()=runTest {
        val current=state();var accepted=0;var checks=0
        val preflight=CreationInputPreflight(this,current,{false},{accepted++},{if(checks++==0) "validation_unavailable" else null},StandardTestDispatcher(testScheduler))
        preflight.submit();runCurrent();val notice=requireNotNull(current.value.inputValidation)
        preflight.updateInput { it.copy(selectedItemId=other.id) }
        preflight.retry(notice);runCurrent()
        assertEquals(0,accepted)
        assertEquals(1,checks)
        assertEquals(listOf(original,other),current.value.items)
    }

    @Test
    fun leavingAndReturningToTheSameSelectionDoesNotReviveItsCheck()=runTest {
        val current=state();val result=CompletableDeferred<String?>();var accepted=0
        val preflight=CreationInputPreflight(this,current,{false},{accepted++},{result.await()},StandardTestDispatcher(testScheduler))
        preflight.submit();runCurrent()
        preflight.updateInput { it.copy(selectedItemId=other.id) }
        preflight.updateInput { it.copy(selectedItemId=original.id) }
        result.complete(null);runCurrent()
        assertEquals(0,accepted)
        assertEquals(listOf(original,other),current.value.items)
        assertNull(current.value.inputValidation)
    }

    @Test
    fun failedCompareAndSetCannotScheduleAnUncommittedAdmission()=runTest {
        val current=state();var accepted=0;var interfere=false
        val preflight=CreationInputPreflight(this,current,{
            if(interfere) { interfere=false;current.value=current.value.copy(selectedItemId=other.id) }
            false
        },{accepted++},{interfere=true;null},StandardTestDispatcher(testScheduler))
        preflight.submit();runCurrent()
        assertEquals(listOf(original,other),current.value.items)
        assertEquals(0,accepted)
    }

    @Test
    fun closingDuringInspectionDoesNotSubmitOrReplacePublishedResults()=runTest {
        val current=state();var closed=false;val result=CompletableDeferred<String?>();var accepted=0
        val preflight=CreationInputPreflight(this,current,{closed},{accepted++},{result.await()},StandardTestDispatcher(testScheduler))
        preflight.submit();runCurrent();closed=true;result.complete(null);runCurrent()
        assertEquals(0,accepted)
        assertEquals(listOf(original,other),current.value.items)
        assertNull(current.value.validatingItemId)
    }

    @Test
    fun configurationRoundTripRetiresTheOldCheck()=runTest {
        val current=state();val result=CompletableDeferred<String?>();var accepted=0
        val preflight=CreationInputPreflight(this,current,{false},{accepted++},{result.await()},StandardTestDispatcher(testScheduler))
        preflight.submit();runCurrent()
        preflight.updateInput { it.copy(outputDirectory="changed") }
        preflight.updateInput { it.copy(outputDirectory="") }
        result.complete(null);runCurrent()
        assertEquals(0,accepted)
        assertEquals(listOf(original,other),current.value.items)
    }

    @Test
    fun closedOwnerRejectsNewTargetMutationsAndChecks()=runTest {
        val current=state();var checks=0
        val preflight=CreationInputPreflight(this,current,{false},{fail("closed submit")},{checks++;null},StandardTestDispatcher(testScheduler))
        preflight.close()
        preflight.updateInput { it.copy(selectedItemId=other.id) }
        preflight.submit();runCurrent()
        assertEquals(original.id,current.value.selectedItemId)
        assertEquals(0,checks)
    }
}

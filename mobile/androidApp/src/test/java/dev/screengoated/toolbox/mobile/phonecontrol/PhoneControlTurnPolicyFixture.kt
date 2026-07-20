package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PHONE_CONTROL_TURN_POLICY
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PHONE_CONTROL_COMPLETION_QUEUE_CAPACITY
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlSessionPayloadQueue
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolFramePreflight
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals

internal fun assertPhoneControlFixturePolicy(expected: JsonObject) {
    val policy = PHONE_CONTROL_TURN_POLICY
    val actual = buildJsonObject {
        put("maximumFinalResponsesPerUserTurn", policy.maximumFinalResponsesPerUserTurn)
        put("maximumAdmittedToolJobs", policy.maximumAdmittedToolJobs)
        put("maximumHeldToolRejections", policy.maximumHeldToolRejections)
        put("rejectionOverflowAbandonsSession", policy.rejectionOverflowAbandonsSession)
        put("maximumSessionControlPayloads", PhoneControlSessionPayloadQueue.MAXIMUM_COUNT)
        put("maximumSessionControlUtf8Bytes", PhoneControlSessionPayloadQueue.MAXIMUM_UTF8_BYTES)
        put("maximumSessionPayloadUtf8Bytes", PhoneControlSessionPayloadQueue.MAXIMUM_PAYLOAD_UTF8_BYTES)
        put("maximumToolCallsPerFrame", PhoneControlToolFramePreflight.MAXIMUM_CALLS)
        put("maximumToolCallIdUtf8Bytes", PhoneControlToolFramePreflight.MAXIMUM_ID_UTF8_BYTES)
        put("maximumToolNameUtf8Bytes", PhoneControlToolFramePreflight.MAXIMUM_NAME_UTF8_BYTES)
        put("maximumToolArgumentsUtf8Bytes", PhoneControlToolFramePreflight.MAXIMUM_ARGUMENTS_UTF8_BYTES)
        put("maximumToolFrameUtf8Bytes", PhoneControlToolFramePreflight.MAXIMUM_FRAME_UTF8_BYTES)
        put("completionQueueCapacity", PHONE_CONTROL_COMPLETION_QUEUE_CAPACITY)
        put("nonresumableReconnectAbandonsOutbound", true)
        put("doneIsTerminal", policy.doneIsTerminal)
        put("cleanupProducesOutput", policy.cleanupProducesOutput)
        put("catalogStableWithinNormalTurns", true)
        put("currentGenerationAudioBlockedByTools", policy.currentGenerationAudioBlockedByTools)
        put("lateRetiredEventsAreAbsorbed", policy.lateRetiredEventsAreAbsorbed)
        put("unknownMutationRequiresReconciliation", policy.unknownMutationRequiresReconciliation)
        put("completionRequiresNoPendingJobs", policy.completionRequiresNoPendingJobs)
        put("reconciliationBlocksMutationAndCompletion", policy.reconciliationBlocksMutationAndCompletion)
        put("browserCredentialsNeverCopiedBetweenProviders", true)
        put("cancellationRequestIsNotTerminalAcknowledgement", policy.cancellationRequestIsNotTerminalAcknowledgement)
        put("acceptedEffectRetainsSingleFlightSlotUntilProviderTerminal", policy.acceptedEffectRetainsSingleFlightSlotUntilProviderTerminal)
        put("onlyTransmittedFreshScreenClearsReconciliation", true)
        put("reconciledScreenDoesNotCompleteActiveGeneration", true)
        put("toolReceiptPrecedesToolOwnedScreenEvidence", true)
        put("ambientScreenBlockedWhileToolResponseOutstanding", true)
        put("microphoneAudioBlockedByTools", false)
        put("transportFailureTailContainsPayloadContent", false)
    }
    assertEquals("Phone Control invariant policy drifted", expected, actual)
}

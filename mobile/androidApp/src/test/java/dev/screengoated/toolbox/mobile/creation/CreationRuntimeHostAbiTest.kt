package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeEngine
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeEventSink
import dev.screengoated.toolbox.mobile.creation.runtime.CreationRuntimeFactory
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationRuntimeHostAbiTest {
    @Test
    fun `runtime installation failure is feature level product copy`() {
        assertEquals(
            "Creation tools could not be installed. Try again.",
            dev.screengoated.toolbox.mobile.creation.runtime.CREATION_RUNTIME_INSTALL_FAILURE,
        )
    }

    @Test
    fun `delivered runtime host method ABI remains exact`() {
        assertEquals(
            mapOf("createEngine" to 3, "runtimeManifest" to 0),
            CreationRuntimeFactory::class.java.declaredMethods.associate {
                it.name to it.parameterCount
            },
        )
        assertEquals(
            mapOf(
                "destroy" to 0,
                "prepare" to 2,
                "runJob" to 3,
                "supportsRequest" to 1,
            ),
            CreationRuntimeEngine::class.java.declaredMethods.associate {
                it.name to it.parameterCount
            },
        )
        assertEquals(
            mapOf("emit" to 1),
            CreationRuntimeEventSink::class.java.declaredMethods.associate {
                it.name to it.parameterCount
            },
        )
    }

    @Test
    fun `worker readiness wire surface stays product only`() {
        val wire = Json.encodeToString(
            CreationWorkerEvent(event = "ready", ready = true),
        )
        val keys = Json.parseToJsonElement(wire).jsonObject.keys

        assertTrue("ready" in keys)
        assertFalse("retryAfterMs" in keys)
        assertFalse("availableModels" in keys)
        assertFalse("ownedJobReady" in keys)
        assertFalse("errorCode" in keys)
        assertFalse("progressText" in keys)
        assertFalse("error" in keys)
        assertFalse("progressKey" in keys)
        assertFalse("phase" in keys)
    }

    @Test
    fun `malformed oversized and missing-ready events fail closed`() {
        assertEquals(null, decodeCreationWorkerEvent("{broken"))
        assertEquals(
            null,
            decodeCreationWorkerEvent("x".repeat(MAXIMUM_CREATION_WORKER_EVENT_BYTES + 1)),
        )
        assertFalse(creationPreparationEventIsReady(CreationWorkerEvent(event = "ready")))
        assertFalse(
            creationPreparationEventIsReady(
                CreationWorkerEvent(event = "ready", ready = false),
            ),
        )
        assertTrue(
            creationPreparationEventIsReady(
                CreationWorkerEvent(event = "ready", ready = true),
            ),
        )
        assertEquals(
            CreationPreparationEventDisposition.IN_PROGRESS,
            creationPreparationEventDisposition(
                CreationWorkerEvent(event = "progress", progressRatio = 0.2),
            ),
        )
        assertEquals(
            CreationPreparationEventDisposition.READY,
            creationPreparationEventDisposition(
                CreationWorkerEvent(event = "ready", ready = true),
            ),
        )
        assertEquals(
            CreationPreparationEventDisposition.RETRY,
            creationPreparationEventDisposition(CreationWorkerEvent(event = "failure")),
        )
        assertEquals(
            CreationPreparationEventDisposition.RETRY,
            creationPreparationEventDisposition(null),
        )
    }
}

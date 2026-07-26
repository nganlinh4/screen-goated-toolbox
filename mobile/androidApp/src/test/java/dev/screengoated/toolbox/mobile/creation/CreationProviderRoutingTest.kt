package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationProviderRoutingTest {
    private val json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    }

    @Test
    fun `explicit generation modes match every shared case`() {
        val fixture = fixture()
        val routing = fixture.getValue("generationModes").jsonObject

        routing.getValue("cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val mode = CreationGenerationMode.fromWireName(case.string("mode"))
            val polycount = case.getValue("polycount").jsonPrimitive.int
            val requestedAutoSegment = case.getValue("autoSegment").jsonPrimitive.boolean
            val route = CreationContract.route3dProvider(mode, polycount, requestedAutoSegment)
            val resultSegmented = case.getValue("resultSegmented").jsonPrimitive.boolean

            assertEquals(mode, route.mode)
            assertEquals(polycount, route.polycount)
            assertEquals(
                case.getValue("effectiveAutoSegment").jsonPrimitive.boolean,
                route.autoSegment,
            )
            assertEquals(
                case.getValue("showAutoSegment").jsonPrimitive.boolean,
                route.showAutoSegment,
            )
            assertEquals(
                case.getValue("canSegmentAfterGeneration").jsonPrimitive.boolean,
                CreationContract.canContinueSegmentation(
                    provider = route.provider.wireName,
                    isSegmented = resultSegmented,
                    runtimeAllowsContinuation = true,
                ),
            )
        }

        routing.getValue("clampingCases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val mode = CreationGenerationMode.fromWireName(case.string("mode"))
            val route = CreationContract.route3dProvider(
                mode,
                case.getValue("polycount").jsonPrimitive.int,
                case.getValue("autoSegment").jsonPrimitive.boolean,
            )
            assertEquals(case.getValue("normalizedPolycount").jsonPrimitive.int, route.polycount)
            assertEquals(
                case.getValue("effectiveAutoSegment").jsonPrimitive.boolean,
                route.autoSegment,
            )
        }
    }

    @Test
    fun `host preserves explicit mode while validating its provider`() {
        val fast = CreationContract.validate3dProvider(
            CreationGenerationMode.FAST,
            20_000,
            true,
            "meshy",
        )
        assertEquals(CreationProvider.MESHY, fast.provider)
        assertEquals(15_000, fast.polycount)
        assertFalse(fast.autoSegment)
        assertFalse(fast.showAutoSegment)

        assertTrue(
            runCatching {
                CreationContract.validate3dProvider(
                    CreationGenerationMode.FAST,
                    5_000,
                    false,
                    "tripo",
                )
            }.isFailure,
        )
        assertTrue(
            runCatching {
                CreationContract.validate3dProvider(
                    CreationGenerationMode.QUALITY,
                    5_000,
                    true,
                    "meshy",
                )
            }.isFailure,
        )
    }

    @Test
    fun `Meshy dispatch stays within its two isolated worker stores`() {
        assertTrue(CreationContract.canUse3dWorker("meshy", 0))
        assertTrue(CreationContract.canUse3dWorker("meshy", 1))
        assertFalse(CreationContract.canUse3dWorker("meshy", 2))
        assertFalse(CreationContract.canUse3dWorker("meshy", 3))
        repeat(CreationContract.IMAGE_TO_3D_WORKSPACES) { slot ->
            assertTrue(CreationContract.canUse3dWorker("tripo", slot))
        }
    }

    @Test
    fun `Meshy recovery redirects only to its valid owner worker`() {
        assertEquals(
            "meshy-recovery-owner:",
            CreationContract.MESHY_RECOVERY_OWNER_PREFIX,
        )
        assertEquals(
            CreationWorkerFailureRoute.Redispatch("3d-0"),
            routeCreationWorkerFailure("meshy", "meshy-recovery-owner:0"),
        )
        assertEquals(
            CreationWorkerFailureRoute.Redispatch("3d-1"),
            routeCreationWorkerFailure("meshy", "meshy-recovery-owner:1"),
        )
        assertEquals(
            CreationWorkerFailureRoute.Fail,
            routeCreationWorkerFailure("tripo", "meshy-recovery-owner:0"),
        )
        assertEquals(
            CreationWorkerFailureRoute.Fail,
            routeCreationWorkerFailure("meshy", "ordinary failure"),
        )
        listOf(
            "meshy-recovery-owner:",
            "meshy-recovery-owner:-1",
            "meshy-recovery-owner:2",
            "meshy-recovery-owner:1:extra",
        ).forEach { error ->
            assertTrue(
                runCatching {
                    routeCreationWorkerFailure("meshy", error)
                }.isFailure,
            )
        }
    }

    @Test
    fun `quality recovery redirects across all four owning workers`() {
        repeat(CreationContract.IMAGE_TO_3D_WORKSPACES) { slot ->
            assertEquals(
                CreationWorkerFailureRoute.Redispatch("3d-$slot"),
                routeCreationWorkerFailure(
                    "tripo",
                    "${CreationContract.TRIPO_RECOVERY_OWNER_PREFIX}$slot",
                ),
            )
        }
        assertTrue(
            runCatching {
                routeCreationWorkerFailure(
                    "tripo",
                    "${CreationContract.TRIPO_RECOVERY_OWNER_PREFIX}4",
                )
            }.isFailure,
        )
    }

    @Test
    fun `creation presentation does not expose provider branding`() {
        val root = repoRoot()
        listOf(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/" +
                "creation/CreationNativeSettings.kt",
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/i18n/" +
                "MobileLocaleEn.kt",
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/i18n/" +
                "MobileLocaleKo.kt",
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/i18n/" +
                "MobileLocaleVi.kt",
        ).forEach { path ->
            val source = File(root, path).readText()
            assertFalse(source.contains("Meshy", ignoreCase = true))
            assertFalse(source.contains("Tripo", ignoreCase = true))
        }
    }

    @Test
    fun `provider branded runtime text is sanitized before presentation`() {
        val text = publicCreationText("Meshy T2 queued; Tripo fallback")
        assertEquals(
            "creation service queued; creation service fallback",
            text,
        )
    }

    @Test
    fun `worker wire models preserve the frozen mode and provider`() {
        val request = CreationWorkerRequest(
            jobId = "job",
            tool = CreationTool.IMAGE_TO_3D.wireName,
            generationMode = CreationGenerationMode.FAST.wireName,
            provider = CreationProvider.MESHY.wireName,
            operation = "generate",
            imagePath = "source.png",
            outputPath = "result.glb",
            outputName = "result.glb",
            polycount = 500,
            autoSegment = false,
        )
        val requestWire = json.encodeToJsonElement(
            CreationWorkerRequest.serializer(),
            request,
        ).jsonObject
        assertEquals("fast", requestWire.string("generationMode"))
        assertEquals("meshy", requestWire.string("provider"))
        assertEquals(
            request,
            json.decodeFromJsonElement(CreationWorkerRequest.serializer(), requestWire),
        )

        val event = CreationWorkerEvent(
            jobId = "job",
            generationMode = CreationGenerationMode.FAST.wireName,
            provider = CreationProvider.MESHY.wireName,
            event = "success",
            isSegmented = true,
            canSegment = false,
            estimatedTotalMs = 90_000,
            timingSampleCount = 4,
        )
        assertEquals(
            "fast",
            json.encodeToJsonElement(CreationWorkerEvent.serializer(), event)
                .jsonObject
                .string("generationMode"),
        )
        assertEquals(
            "meshy",
            json.encodeToJsonElement(CreationWorkerEvent.serializer(), event)
                .jsonObject
                .string("provider"),
        )
        assertEquals(
            4,
            json.encodeToJsonElement(CreationWorkerEvent.serializer(), event)
                .jsonObject
                .getValue("timingSampleCount")
                .jsonPrimitive
                .int,
        )

        val status = CreationJobStatus(
            jobId = "job",
            generationMode = CreationGenerationMode.FAST.wireName,
            provider = CreationProvider.MESHY.wireName,
            polycount = 500,
            autoSegment = false,
            stage = "done",
            progressText = "Model ready.",
            isSegmented = true,
            canSegment = false,
        )
        val statusWire = json.encodeToJsonElement(
            CreationJobStatus.serializer(),
            status,
        ).jsonObject
        assertEquals("fast", statusWire.string("generationMode"))
        assertEquals("meshy", statusWire.string("provider"))
        assertEquals(500, statusWire.getValue("polycount").jsonPrimitive.int)
        assertFalse(statusWire.getValue("autoSegment").jsonPrimitive.boolean)
    }

    private fun fixture() = json.parseToJsonElement(
        File(repoRoot(), "parity-fixtures/image-to-3d/state-contract.json").readText(),
    ).jsonObject

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { it.parentFile }
            .firstOrNull { File(it, "parity-fixtures").isDirectory }
            ?: error("Could not locate the repository from $workingDirectory")
    }

    private fun kotlinx.serialization.json.JsonObject.string(key: String): String =
        getValue(key).jsonPrimitive.contentOrNull ?: error("$key is not a string")
}

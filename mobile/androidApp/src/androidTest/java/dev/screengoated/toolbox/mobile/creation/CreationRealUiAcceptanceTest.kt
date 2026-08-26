package dev.screengoated.toolbox.mobile.creation

import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.net.Uri
import android.os.Environment
import android.os.SystemClock
import android.provider.MediaStore
import android.provider.OpenableColumns
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.UiDevice
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import java.io.File
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assume.assumeTrue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

@RunWith(AndroidJUnit4::class)
class CreationRealUiAcceptanceTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
    private var sourceUri: Uri? = null

    @Before
    fun resetFreshCreationState() {
        check(CreationJobJournal(context).load().none { creationStageIsBusy(it.status.stage) }) {
            "Fresh acceptance reset refused while a creation dispatch is recoverable"
        }
        resetCreationFilesPreservingInstalledRuntime()
        resetWorkerBrowserProfiles()
        context.contentResolver.query(
            MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
            arrayOf(MediaStore.Images.Media._ID),
            "${MediaStore.Images.Media.DISPLAY_NAME} LIKE ?",
            arrayOf("$INPUT_NAME_PREFIX%"),
            null,
        )?.use { cursor ->
            val idColumn = cursor.getColumnIndexOrThrow(MediaStore.Images.Media._ID)
            while (cursor.moveToNext()) {
                val uri = Uri.withAppendedPath(
                    MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
                    cursor.getLong(idColumn).toString(),
                )
                context.contentResolver.delete(uri, null, null)
            }
        }
    }

    private fun resetCreationFilesPreservingInstalledRuntime() {
        val creationRoot = File(context.filesDir, "creation")
        creationRoot.listFiles().orEmpty()
            .filterNot { it.name == "runtime" }
            .forEach { path ->
                check(path.deleteRecursively()) {
                    "Could not remove prior creation acceptance state: ${path.name}"
                }
            }
        val cache = File(context.cacheDir, "creation")
        check(!cache.exists() || cache.deleteRecursively()) {
            "Could not remove prior creation acceptance cache"
        }
    }

    private fun resetWorkerBrowserProfiles() {
        context.dataDir.listFiles().orEmpty()
            .filter { it.name.startsWith(WORKER_WEBVIEW_DIRECTORY_PREFIX) }
            .forEach { path ->
                check(path.deleteRecursively()) {
                    "Could not remove prior worker browser profile: ${path.name}"
                }
            }
    }

    @After
    fun removeInput() {
        sourceUri?.let { context.contentResolver.delete(it, null, null) }
        sourceUri = null
    }

    @Test
    fun imageTo3dQualityGeneratesValidatedGlb() {
        runImageCase(
            "3d-quality",
            CreationTool.IMAGE_TO_3D,
            null,
            CreationGenerationMode.QUALITY,
            expectedAutoSegment = true,
        ) {
            CreationArtifactValidator.validateGlb(it)
        }
    }

    @Test
    fun imageToSvgSimpleGeneratesValidatedSvg() {
        runImageCase("svg", CreationTool.IMAGE_TO_SVG, "creation-svg-simple", null) {
            CreationArtifactValidator.validateSvg(it)
        }
    }

    @Test
    fun imageCreatorTextOnlyGeneratesValidatedPng() {
        assumeTrue(
            "Image creation restoration case is not released",
            creationToolReleased(CreationTool.IMAGE_CREATOR),
        )
        runSurface(CreationTool.IMAGE_CREATOR) { startedAt, ownerId ->
            compose.onNodeWithTag("creation-image-prompt")
                .assertExists()
                .performTextInput("Create a cobalt circle on a warm neutral background")
            compose.onNodeWithTag("creation-primary-action").assertIsEnabled()
            submitAndValidate(
                "image-text",
                CreationTool.IMAGE_CREATOR,
                ownerId,
                startedAt,
                null,
                emptyList(),
            ) {
                CreationArtifactValidator.validatePng(it, null, null)
            }
        }
    }

    @Test
    fun imageCreatorReferenceEditGeneratesValidatedPng() {
        assumeTrue(
            "Image creation restoration case is not released",
            creationToolReleased(CreationTool.IMAGE_CREATOR),
        )
        sourceUri = createInputImage()
        runSurface(CreationTool.IMAGE_CREATOR) { startedAt, ownerId ->
            compose.onNodeWithTag("creation-image-add-references")
                .assertExists()
                .performScrollTo()
                .assertIsDisplayed()
                .assertIsEnabled()
                .performClick()
            selectInputFromSystemPicker(requireNotNull(sourceUri))
            compose.waitUntil(timeoutMillis = 30_000) {
                compose.onAllNodesWithTag("creation-image-reference-0")
                    .fetchSemanticsNodes(atLeastOneRootRequired = false)
                    .isNotEmpty()
            }
            compose.onNodeWithTag("creation-image-prompt")
                .assertExists()
                .performTextInput("Turn this into a crisp coral and cobalt poster")
            compose.onNodeWithTag("creation-primary-action").assertIsEnabled()
            submitAndValidate(
                "image-reference-edit",
                CreationTool.IMAGE_CREATOR,
                ownerId,
                startedAt,
                null,
                listOf(sha256(requireNotNull(sourceUri))),
            ) {
                CreationArtifactValidator.validatePng(it, null, null)
            }
        }
    }

    private fun runImageCase(
        caseName: String,
        tool: CreationTool,
        settingTag: String?,
        expectedGenerationMode: CreationGenerationMode?,
        expectedAutoSegment: Boolean = false,
        validate: (File) -> Unit,
    ) {
        sourceUri = createInputImage()
        runSurface(tool) { startedAt, ownerId ->
            val queueSizeBeforeImport = currentQueueSize()
            compose.onNodeWithTag("creation-add-input").assertExists().performClick()
            selectInputFromSystemPicker(requireNotNull(sourceUri))
            compose.waitUntil(timeoutMillis = 30_000) {
                currentQueueSize() > queueSizeBeforeImport &&
                    compose.onAllNodesWithTag("creation-selected-stage-draft")
                        .fetchSemanticsNodes(atLeastOneRootRequired = false)
                        .isNotEmpty()
            }
            compose.onNodeWithTag("creation-selected-input").assertExists()
            compose.onNodeWithTag("creation-selected-stage-draft").assertExists()
            settingTag?.let { tag ->
                val setting = compose.onNodeWithTag(tag)
                    .assertExists()
                    .performScrollTo()
                    .assertIsDisplayed()
                    .assertIsEnabled()
                setting.performClick()
                if (expectedGenerationMode != null) setting.assertIsOn()
            }
            if (expectedAutoSegment) {
                compose.onNodeWithTag("creation-auto-separate")
                    .assertExists()
                    .performScrollTo()
                    .assertIsDisplayed()
                    .assertIsEnabled()
                    .performClick()
            }
            compose.onNodeWithTag("creation-primary-action").assertIsEnabled()
            submitAndValidate(
                caseName,
                tool,
                ownerId,
                startedAt,
                expectedGenerationMode,
                listOf(sha256(requireNotNull(sourceUri))),
                expectedAutoSegment,
                validate,
            )
        }
    }

    private fun runSurface(
        tool: CreationTool,
        body: (Long, String) -> Unit,
    ) {
        val startedAt = System.currentTimeMillis()
        val ownerId = "$QUALITY_CONTROL_OWNER_PREFIX${UUID.randomUUID()}"
        val intent = Intent(context, CreationMiniAppActivity::class.java)
            .putExtra("creation_tool", tool.wireName)
            .putExtra("creation_owner_id", ownerId)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)
        ActivityScenario.launch<CreationMiniAppActivity>(intent).use {
            compose.onNodeWithTag("creation-root").assertExists()
            body(startedAt, ownerId)
        }
    }

    private fun submitAndValidate(
        caseName: String,
        tool: CreationTool,
        ownerId: String,
        startedAt: Long,
        expectedGenerationMode: CreationGenerationMode?,
        expectedReferenceSha256: List<String>,
        expectedAutoSegment: Boolean = false,
        validate: (File) -> Unit,
    ) {
        compose.onNodeWithTag("creation-primary-action").assertExists().performClick()
        val acceptedCase = awaitWorkerAssignment(ownerId, tool)
        assertEquals(
            "Accepted automatic separation setting changed",
            expectedAutoSegment,
            acceptedCase.request.autoSegment,
        )
        val requestEvidence = CreationAcceptanceAttestation.acceptedRequest(
            context,
            acceptedCase,
            expectedGenerationMode,
            expectedReferenceSha256,
        )
        val history = awaitCommittedHistory(tool, acceptedCase, startedAt)
        val output = snapshotOutput(history.outputPath)
        assertTrue("Committed result does not exist", outputExists(history.outputPath))
        validate(output)
        val artifactSha256 = outputSha256(history.outputPath)
        assertEquals("Committed artifact SHA-256 changed", history.committedSha256, artifactSha256)
        assertEquals(
            "Committed artifact size changed",
            history.committedSize,
            outputSize(history.outputPath),
        )
        val segmented = if (expectedAutoSegment) {
            assertNull(
                "A separation-compatible triangle base published a quad companion",
                history.metadata["download"],
            )
            awaitAutomaticSegmentation(
                ownerId, acceptedCase, history, history.outputPath, artifactSha256,
            )
        } else {
            null
        }
        compose.waitUntil(timeoutMillis = 10_000) {
            compose.onAllNodesWithTag("creation-selected-stage-done")
                .fetchSemanticsNodes(atLeastOneRootRequired = false)
                .isNotEmpty()
        }
        compose.onNodeWithTag("creation-selected-stage-done").assertExists()
        val delivery = CreationAcceptanceAttestation.selectedRuntime(context)
        val arguments = InstrumentationRegistry.getArguments()
        assertEquals(
            "Instrumentation case selection changed",
            caseName,
            requireNotNull(arguments.getString("sgtCreationCaseName")),
        )
        val caseId = requireNotNull(arguments.getString("sgtCreationCaseId"))
        println(
            "$ACCEPTANCE_EVIDENCE_PREFIX " +
                JSONObject()
                    .put("schemaVersion", 2)
                    .put("caseName", caseName)
                    .put("caseId", caseId)
                    .put("distribution", delivery.distribution)
                    .put("ownerId", ownerId)
                    .put("engineId", requireNotNull(acceptedCase.engineId))
                    .put("dispatchId", acceptedCase.request.dispatchId)
                    .put("tool", tool.wireName)
                    .put("acceptedRequestFingerprint", requestEvidence.fingerprint)
                    .put(
                        "acceptedGenerationMode",
                        requestEvidence.generationMode ?: JSONObject.NULL,
                    )
                    .put("acceptedReferenceCount", requestEvidence.referenceSha256.size)
                    .put("acceptedReferenceSha256", JSONArray(requestEvidence.referenceSha256))
                    .put("acceptedRequestValidated", requestEvidence.frozenAndValidated)
                    .put("artifactSize", outputSize(history.outputPath))
                    .put("artifactSha256", artifactSha256)
                    .put("artifactValidator", "project-structural-v1")
                    .put("basePublishedBeforeAutomaticSegmentation", segmented != null)
                    .put("segmentedArtifactSha256", segmented?.let(::sha256) ?: JSONObject.NULL)
                    .put("deliveryChannel", delivery.channel)
                    .put("contractSha256", delivery.contractSha256)
                    .put("runtimeArtifactSha256", delivery.runtimeArtifactSha256)
                    .put("runtimeFactoryClass", delivery.runtimeFactoryClass)
                    .put("runtimeVersion", delivery.runtimeVersion)
                    .put("runtimeManifestSha256", delivery.runtimeManifestSha256)
                    .put("runtimeSplitName", delivery.runtimeSplitName ?: JSONObject.NULL)
                    .put("mailboxPollIntervalMs", delivery.mailboxPollIntervalMs)
                    .toString(),
        )
    }

    private fun awaitAutomaticSegmentation(
        ownerId: String,
        baseRecord: CreationJournalRecord,
        baseHistory: CreationHistoryEntry,
        basePath: String,
        baseSha256: String,
    ): File {
        val deadline = SystemClock.elapsedRealtime() + MAXIMUM_CASE_RUNTIME_MS
        while (SystemClock.elapsedRealtime() < deadline) {
            val child = CreationJobJournal(context).load().firstOrNull { record ->
                record.ownerId == ownerId &&
                    record.request.operation == "refine" &&
                    record.request.refinementKind == "separate_detailed" &&
                    record.request.parentRevisionId == baseRecord.request.dispatchId &&
                    record.request.previousOutputPath == baseHistory.outputPath
            }
            if (child != null) {
                check(child.request.dispatchId != baseRecord.request.dispatchId)
                check(child.status.stage !in TERMINAL_FAILURE_STAGES) {
                    "Automatic separation reached a failed terminal state"
                }
                CreationJobManager.get(context).history.list(CreationTool.IMAGE_TO_3D)
                    .firstOrNull { it.dispatchId == child.request.dispatchId }
                    ?.let { history ->
                        assertEquals(true, history.metadata["isSegmented"]?.jsonPrimitive?.booleanOrNull)
                        assertTrue(
                            "Segmented result advertised an unsupported action",
                            history.metadata["supportedActions"]?.jsonArray?.isEmpty() == true,
                        )
                        assertTrue(
                            "Segmented result retained an unavailable continuation",
                            history.metadata["availableActions"]?.jsonArray?.isEmpty() == true,
                        )
                        val result = snapshotOutput(history.outputPath)
                        assertTrue(
                            "Segmented result does not exist",
                            outputExists(history.outputPath),
                        )
                        CreationArtifactValidator.validateGlb(result)
                        assertTrue(
                            "Base result was removed by automatic separation",
                            outputExists(basePath),
                        )
                        assertEquals(baseSha256, outputSha256(basePath))
                        return result
                    }
            }
            SystemClock.sleep(ACCEPTANCE_POLL_INTERVAL_MS)
        }
        error("Automatic separation did not commit before its whole-job deadline")
    }

    private fun awaitWorkerAssignment(
        ownerId: String,
        tool: CreationTool,
    ): CreationJournalRecord {
        val deadline = SystemClock.elapsedRealtime() + MAXIMUM_CASE_RUNTIME_MS
        while (SystemClock.elapsedRealtime() < deadline) {
            val current = CreationJobJournal(context).load().singleOrNull { record ->
                record.ownerId == ownerId && record.request.tool == tool.wireName
            }
            if (current != null) {
                check(current.status.stage !in TERMINAL_FAILURE_STAGES) {
                    "Creation failed before worker assignment"
                }
                if (current.engineId != null) return current
            }
            SystemClock.sleep(ACCEPTANCE_POLL_INTERVAL_MS)
        }
        error("Creation did not receive a worker before its whole-job deadline")
    }

    private fun awaitCommittedHistory(
        tool: CreationTool,
        accepted: CreationJournalRecord,
        startedAt: Long,
    ): CreationHistoryEntry {
        val deadline = SystemClock.elapsedRealtime() + MAXIMUM_CASE_RUNTIME_MS
        while (SystemClock.elapsedRealtime() < deadline) {
            CreationJobManager.get(context).history.list(tool).firstOrNull {
                it.createdAtMs >= startedAt && it.dispatchId == accepted.request.dispatchId
            }?.let { return it }
            val current = CreationJobJournal(context).load().singleOrNull {
                it.request.dispatchId == accepted.request.dispatchId
            }
            check(current == null || current.status.stage !in TERMINAL_FAILURE_STAGES) {
                "Creation reached a failed terminal state"
            }
            SystemClock.sleep(ACCEPTANCE_POLL_INTERVAL_MS)
        }
        error("Creation did not commit a result before its whole-job deadline")
    }

    private fun createInputImage(): Uri {
        val displayName = "$INPUT_NAME_PREFIX${UUID.randomUUID()}.png"
        val values = ContentValues().apply {
            put(MediaStore.Images.Media.DISPLAY_NAME, displayName)
            put(MediaStore.Images.Media.MIME_TYPE, "image/png")
            put(
                MediaStore.Images.Media.RELATIVE_PATH,
                Environment.DIRECTORY_PICTURES,
            )
            put(MediaStore.Images.Media.IS_PENDING, 1)
        }
        val uri = requireNotNull(
            context.contentResolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values),
        )
        val bitmap = Bitmap.createBitmap(512, 512, Bitmap.Config.ARGB_8888)
        Canvas(bitmap).run {
            drawColor(Color.rgb(246, 244, 239))
            val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                color = Color.rgb(34, 104, 170)
                style = Paint.Style.FILL
            }
            drawRoundRect(76f, 116f, 436f, 396f, 42f, 42f, paint)
            paint.color = Color.WHITE
            drawCircle(256f, 256f, 84f, paint)
        }
        context.contentResolver.openOutputStream(uri, "w").use {
            checkNotNull(it)
            check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it))
        }
        bitmap.recycle()
        values.clear()
        values.put(MediaStore.Images.Media.IS_PENDING, 0)
        context.contentResolver.update(uri, values, null, null)
        return uri
    }

    private fun sha256(uri: Uri): String = context.contentResolver.openInputStream(uri).use {
        sha256(requireNotNull(it).readBytes())
    }

    private fun outputExists(path: String): Boolean = path.creationContentUri()?.let { uri ->
        runCatching {
            context.contentResolver.openAssetFileDescriptor(uri, "r")?.use { true } ?: false
        }.getOrDefault(false)
    } ?: File(path).isFile

    private fun outputSize(path: String): Long = path.creationContentUri()?.let { uri ->
        context.contentResolver.query(
            uri,
            arrayOf(OpenableColumns.SIZE),
            null,
            null,
            null,
        )?.use { cursor -> if (cursor.moveToFirst()) cursor.getLong(0) else -1L } ?: -1L
    } ?: File(path).length()

    private fun outputDisplayName(path: String): String = path.creationContentUri()?.let { uri ->
        context.contentResolver.query(
            uri,
            arrayOf(OpenableColumns.DISPLAY_NAME),
            null,
            null,
            null,
        )?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
    } ?: File(path).name

    private fun outputSha256(path: String): String = path.creationContentUri()?.let(::sha256)
        ?: sha256(File(path))

    private fun snapshotOutput(path: String): File {
        val uri = path.creationContentUri() ?: return File(path)
        val directory = File(context.cacheDir, "creation/acceptance").apply(File::mkdirs)
        val target = File(directory, safeCreationOutputName(outputDisplayName(path)))
        context.contentResolver.openInputStream(uri).use { input ->
            requireNotNull(input).use { source -> target.outputStream().use(source::copyTo) }
        }
        return target
    }

    private fun currentQueueSize(): Int =
        compose.onAllNodesWithTag("creation-input")
            .fetchSemanticsNodes(atLeastOneRootRequired = false).size +
            compose.onAllNodesWithTag("creation-selected-input")
                .fetchSemanticsNodes(atLeastOneRootRequired = false).size

    private fun selectInputFromSystemPicker(uri: Uri) {
        CreationSystemPickerDriver(context, device).select(uri)
        compose.waitUntil(timeoutMillis = 10_000) {
            runCatching {
                compose.onAllNodesWithTag("creation-root")
                    .fetchSemanticsNodes(atLeastOneRootRequired = false)
                    .isNotEmpty()
            }.getOrDefault(false)
        }
        compose.onNodeWithTag("creation-root").assertExists()
    }

    private companion object {
        const val ACCEPTANCE_EVIDENCE_PREFIX = "SGT_CREATION_ACCEPTANCE_EVIDENCE"
        const val INPUT_NAME_PREFIX = "sgt-creation-qc-"
        const val QUALITY_CONTROL_OWNER_PREFIX = "quality-control-"
        const val WORKER_WEBVIEW_DIRECTORY_PREFIX = "app_webview_sgt_creation_"
        const val ACCEPTANCE_POLL_INTERVAL_MS = 500L
        const val MAXIMUM_CASE_RUNTIME_MS = 2L * 60 * 60 * 1_000
        val TERMINAL_FAILURE_STAGES = setOf("failed", "cancelled")
    }

}

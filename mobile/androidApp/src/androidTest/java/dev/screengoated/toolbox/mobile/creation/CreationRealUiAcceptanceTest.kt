package dev.screengoated.toolbox.mobile.creation

import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Rect
import android.net.Uri
import android.os.Environment
import android.os.SystemClock
import android.provider.MediaStore
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.StaleObjectException
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
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
import java.util.regex.Pattern
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assume.assumeTrue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

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
    fun imageTo3dFastGeneratesValidatedGlb() {
        runImageCase(
            "3d-fast",
            CreationTool.IMAGE_TO_3D,
            "creation-mode-fast",
            CreationGenerationMode.FAST,
        ) {
            CreationArtifactValidator.validateGlb(it)
        }
    }

    @Test
    fun imageTo3dQualityGeneratesValidatedGlb() {
        runImageCase(
            "3d-quality",
            CreationTool.IMAGE_TO_3D,
            "creation-mode-quality",
            CreationGenerationMode.QUALITY,
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
        settingTag: String,
        expectedGenerationMode: CreationGenerationMode?,
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
            val setting = compose.onNodeWithTag(settingTag)
                .assertExists()
                .performScrollTo()
                .assertIsDisplayed()
                .assertIsEnabled()
            setting.performClick()
            if (expectedGenerationMode != null) setting.assertIsOn()
            compose.onNodeWithTag("creation-primary-action").assertIsEnabled()
            submitAndValidate(
                caseName,
                tool,
                ownerId,
                startedAt,
                expectedGenerationMode,
                listOf(sha256(requireNotNull(sourceUri))),
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
        validate: (File) -> Unit,
    ) {
        compose.onNodeWithTag("creation-primary-action").assertExists().performClick()
        val acceptedCase = awaitWorkerAssignment(ownerId, tool)
        val requestEvidence = CreationAcceptanceAttestation.acceptedRequest(
            context,
            acceptedCase,
            expectedGenerationMode,
            expectedReferenceSha256,
        )
        val history = awaitCommittedHistory(tool, acceptedCase, startedAt)
        compose.waitUntil(timeoutMillis = 10_000) {
            compose.onAllNodesWithTag("creation-selected-stage-done")
                .fetchSemanticsNodes(atLeastOneRootRequired = false)
                .isNotEmpty()
        }
        compose.onNodeWithTag("creation-selected-stage-done").assertExists()
        val output = File(history.outputPath)
        assertTrue("Committed result does not exist", output.isFile)
        validate(output)
        val artifactSha256 = sha256(output)
        assertEquals("Committed artifact SHA-256 changed", history.committedSha256, artifactSha256)
        assertEquals("Committed artifact size changed", history.committedSize, output.length())
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
                    .put("artifactSize", output.length())
                    .put("artifactSha256", artifactSha256)
                    .put("artifactValidator", "project-structural-v1")
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

    private fun currentQueueSize(): Int =
        compose.onAllNodesWithTag("creation-input")
            .fetchSemanticsNodes(atLeastOneRootRequired = false).size +
            compose.onAllNodesWithTag("creation-selected-input")
                .fetchSemanticsNodes(atLeastOneRootRequired = false).size

    private fun selectInputFromSystemPicker(uri: Uri) {
        val displayName = context.contentResolver.query(
            uri,
            arrayOf(MediaStore.Images.Media.DISPLAY_NAME),
            null,
            null,
            null,
        )!!.use { cursor ->
            check(cursor.moveToFirst())
            cursor.getString(0)
        }
        val bounds = clickPickerTile(displayName)
        device.waitForIdle(1_000)
        confirmPickerSelectionIfPresent()
        if (!waitForCreationAppForeground(3_000)) {
            val recoveredIntoApp = recoverUnresponsiveSystemPicker() &&
                waitForCreationAppForeground(5_000)
            if (!recoveredIntoApp) {
                clickPickerTile(displayName)
                device.waitForIdle(1_000)
                confirmPickerSelectionIfPresent()
            }
        }
        if (!waitForCreationAppForeground(10_000) &&
            recoverUnresponsiveSystemPicker()
        ) {
            waitForCreationAppForeground(10_000)
        }
        check(waitForCreationAppForeground(20_000)) {
            "Creation app did not regain focus after choosing the quality-control image; " +
                "foreground package=${device.currentPackageName}, tileBounds=$bounds"
        }
        compose.waitUntil(timeoutMillis = 10_000) {
            runCatching {
                compose.onAllNodesWithTag("creation-root")
                    .fetchSemanticsNodes(atLeastOneRootRequired = false)
                    .isNotEmpty()
            }.getOrDefault(false)
        }
        compose.onNodeWithTag("creation-root").assertExists()
    }

    private fun waitForCreationAppForeground(timeoutMillis: Long): Boolean {
        val deadline = SystemClock.elapsedRealtime() + timeoutMillis
        while (SystemClock.elapsedRealtime() < deadline) {
            if (device.currentPackageName == context.packageName) return true
            SystemClock.sleep(100)
        }
        return device.currentPackageName == context.packageName
    }

    private fun clickPickerTile(displayName: String): Rect {
        val description = By.desc(Pattern.compile("^${Pattern.quote(displayName)},.*"))
        val text = By.text(displayName)
        val deadline = SystemClock.elapsedRealtime() + 60_000
        while (SystemClock.elapsedRealtime() < deadline) {
            val target = device.findObject(description) ?: device.findObject(text)
            if (target != null) {
                try {
                    val bounds = target.visibleBounds
                    if (!bounds.isEmpty) {
                        target.longClick()
                        return bounds
                    }
                } catch (_: StaleObjectException) {
                    // The picker replaces thumbnail nodes while their previews are decoded.
                }
            }
            SystemClock.sleep(250)
        }
        error("System picker did not show a stable, visible quality-control image tile")
    }

    private fun confirmPickerSelectionIfPresent() {
        val selectAction = device.wait(
            Until.findObject(By.res(SYSTEM_PICKER_PACKAGE, "action_menu_select")),
            5_000,
        )
        if (selectAction != null) {
            selectAction.click()
            device.waitForIdle(1_000)
            return
        }
        val labels = listOf("Open", "Select", "Add", "Done")
        val confirmation = labels.firstNotNullOfOrNull { label ->
            device.wait(Until.findObject(By.text(label)), 2_000)
        }
        confirmation?.click()
        device.waitForIdle(1_000)
    }

    private fun recoverUnresponsiveSystemPicker(): Boolean {
        val wait = device.wait(Until.findObject(By.res("android", "aerr_wait")), 3_000)
            ?: return false
        wait.click()
        device.waitForIdle(2_000)
        return true
    }

    private companion object {
        const val ACCEPTANCE_EVIDENCE_PREFIX = "SGT_CREATION_ACCEPTANCE_EVIDENCE"
        const val INPUT_NAME_PREFIX = "sgt-creation-qc-"
        const val SYSTEM_PICKER_PACKAGE = "com.google.android.documentsui"
        const val QUALITY_CONTROL_OWNER_PREFIX = "quality-control-"
        const val WORKER_WEBVIEW_DIRECTORY_PREFIX = "app_webview_sgt_creation_"
        const val ACCEPTANCE_POLL_INTERVAL_MS = 500L
        const val MAXIMUM_CASE_RUNTIME_MS = 2L * 60 * 60 * 1_000
        val TERMINAL_FAILURE_STAGES = setOf("failed", "cancelled")
    }

}

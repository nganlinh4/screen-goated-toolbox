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
import android.provider.MediaStore
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.swipeUp
import dev.screengoated.toolbox.mobile.MainActivity
import java.io.File
import java.util.UUID
import java.util.regex.Pattern
import org.junit.After
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
    fun removeOrphanedInputs() {
        cancelAbandonedQualityControlJobs()
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

    private fun cancelAbandonedQualityControlJobs() {
        val manager = CreationJobManager.get(context)
        CreationJobJournal(context).load()
            .filter { record ->
                record.ownerId.startsWith(QUALITY_CONTROL_OWNER_PREFIX) &&
                    creationStageIsBusy(record.status.stage)
            }
            .forEach { record ->
                val tool = CreationTool.fromWireName(record.request.tool) ?: return@forEach
                manager.cancel(record.ownerId, tool, record.request.jobId)
            }
    }

    @After
    fun removeInput() {
        sourceUri?.let { context.contentResolver.delete(it, null, null) }
        sourceUri = null
    }

    @Test
    fun imageTo3dFastGeneratesValidatedGlb() {
        runImageCase(CreationTool.IMAGE_TO_3D, "creation-mode-fast") {
            CreationArtifactValidator.validateGlb(it)
        }
    }

    @Test
    fun imageTo3dQualityGeneratesValidatedGlb() {
        runImageCase(CreationTool.IMAGE_TO_3D, "creation-mode-quality") {
            CreationArtifactValidator.validateGlb(it)
        }
    }

    @Test
    fun imageToSvgSimpleGeneratesValidatedSvg() {
        runImageCase(CreationTool.IMAGE_TO_SVG, "creation-svg-simple") {
            CreationArtifactValidator.validateSvg(it)
        }
    }

    @Test
    fun imageCreatorReleaseEntryShowsComingSoonWithoutStartingReadiness() {
        ActivityScenario.launch(MainActivity::class.java).use {
            compose.onNodeWithTag("shell-tab-apps").performClick()
            compose.waitUntil(timeoutMillis = 10_000) {
                compose.onAllNodesWithTag("shell-section-apps")
                    .fetchSemanticsNodes(atLeastOneRootRequired = false)
                    .isNotEmpty()
            }
            scrollAppsTo("app-card-image-creator")
            compose.onNodeWithTag("app-card-image-creator")
                .assertIsDisplayed()
                .performClick()
            compose.onNodeWithTag("image-creator-coming-soon-dialog").assertIsDisplayed()
            assertTrue(
                compose.onAllNodesWithTag("creation-root")
                    .fetchSemanticsNodes(atLeastOneRootRequired = false)
                    .isEmpty(),
            )
            check(
                CreationJobManager.get(context)
                    .preparationStatus(CreationTool.IMAGE_CREATOR) == "unavailable",
            ) { "Disabled image entry started readiness" }
        }
    }

    private fun scrollAppsTo(tag: String) {
        repeat(8) {
            if (
                compose.onAllNodesWithTag(tag)
                    .fetchSemanticsNodes(atLeastOneRootRequired = false)
                    .isNotEmpty()
            ) return
            compose.onNodeWithTag("apps-carousel").performTouchInput {
                if (
                    context.resources.configuration.orientation ==
                    android.content.res.Configuration.ORIENTATION_LANDSCAPE
                ) {
                    swipeLeft(durationMillis = 800)
                } else {
                    swipeUp(durationMillis = 800)
                }
            }
            compose.waitForIdle()
        }
        error("Timed out scrolling to app card: $tag")
    }

    private fun runImageCase(
        tool: CreationTool,
        settingTag: String,
        validate: (File) -> Unit,
    ) {
        sourceUri = createInputImage()
        runSurface(tool) { startedAt ->
            compose.onNodeWithTag("creation-add-input").assertExists().performClick()
            selectInputFromSystemPicker(requireNotNull(sourceUri))
            compose.waitUntil(timeoutMillis = 30_000) {
                compose.onAllNodesWithTag("creation-selected-input")
                    .fetchSemanticsNodes(atLeastOneRootRequired = false)
                    .isNotEmpty()
            }
            compose.onNodeWithTag(settingTag).assertExists().performClick()
            submitAndValidate(tool, startedAt, validate)
        }
    }

    private fun runSurface(
        tool: CreationTool,
        body: (Long) -> Unit,
    ) {
        val startedAt = System.currentTimeMillis()
        val intent = Intent(context, CreationMiniAppActivity::class.java)
            .putExtra("creation_tool", tool.wireName)
            .putExtra("creation_owner_id", "$QUALITY_CONTROL_OWNER_PREFIX${UUID.randomUUID()}")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)
        ActivityScenario.launch<CreationMiniAppActivity>(intent).use {
            compose.onNodeWithTag("creation-root").assertExists()
            body(startedAt)
        }
    }

    private fun submitAndValidate(
        tool: CreationTool,
        startedAt: Long,
        validate: (File) -> Unit,
    ) {
        compose.onNodeWithTag("creation-primary-action").assertExists().performClick()
        compose.waitUntil(timeoutMillis = MAXIMUM_CASE_RUNTIME_MS) {
            val done = compose.onAllNodesWithTag("creation-selected-stage-done")
                .fetchSemanticsNodes(atLeastOneRootRequired = false)
                .isNotEmpty()
            val failed = compose.onAllNodesWithTag("creation-selected-stage-failed")
                .fetchSemanticsNodes(atLeastOneRootRequired = false)
                .isNotEmpty()
            done || failed
        }
        val failed = compose.onAllNodesWithTag("creation-selected-stage-failed")
            .fetchSemanticsNodes(atLeastOneRootRequired = false)
            .isNotEmpty()
        check(!failed) { "Creation reached a failed terminal state" }
        val history = CreationJobManager.get(context).history.list(tool)
            .firstOrNull { it.createdAtMs >= startedAt }
            ?: error("Creation completed without a new committed history result")
        val output = File(history.outputPath)
        assertTrue("Committed result does not exist", output.isFile)
        validate(output)
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
        val target = device.wait(
            Until.findObject(
                By.desc(Pattern.compile("^${Pattern.quote(displayName)},.*")),
            ),
            50_000,
        ) ?: device.wait(Until.findObject(By.text(displayName)), 10_000)
            ?: error("System picker did not show the prepared quality-control image")
        val bounds = target.visibleBounds
        check(!bounds.isEmpty) {
            "System picker showed the quality-control image outside the visible viewport"
        }
        val selectionX = bounds.left + bounds.width() / 2
        val selectionY = bounds.top + bounds.height() * 3 / 4
        check(device.click(selectionX, selectionY)) {
            "System picker could not tap the quality-control image at " +
                "($selectionX, $selectionY), tileBounds=$bounds"
        }
        device.waitForIdle(1_000)
        if (!device.wait(Until.hasObject(By.pkg(context.packageName)), 3_000)) {
            confirmPickerSelectionIfPresent()
        }
        check(device.wait(Until.hasObject(By.pkg(context.packageName)), 10_000)) {
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

    private fun confirmPickerSelectionIfPresent() {
        val labels = listOf("Open", "Select", "Add", "Done")
        val confirmation = labels.firstNotNullOfOrNull { label ->
            device.wait(Until.findObject(By.text(label)), 2_000)
        }
        confirmation?.click()
        device.waitForIdle(1_000)
    }

    private companion object {
        const val INPUT_NAME_PREFIX = "sgt-creation-qc-"
        const val QUALITY_CONTROL_OWNER_PREFIX = "quality-control-"
        const val MAXIMUM_CASE_RUNTIME_MS = 2L * 60 * 60 * 1_000
    }
}

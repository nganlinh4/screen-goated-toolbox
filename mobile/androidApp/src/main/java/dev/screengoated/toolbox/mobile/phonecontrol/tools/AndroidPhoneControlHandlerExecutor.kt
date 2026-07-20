package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import kotlin.math.roundToLong
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonObject

internal class AndroidPhoneControlHandlerExecutor(
    context: Context,
) : PhoneControlHandlerExecutor {
    private val providers = AndroidProviderToolHandlers(context)
    private val detector = UiDetectorToolHandlers(context)
    private val visual = VisualToolHandlers()
    private val surfaces = SurfaceToolHandlers(context)
    private val memory = MemoryToolHandlers(context)
    private val commands = AndroidCommandToolBackend(context)

    override suspend fun execute(
        handler: PhoneControlHandler,
        job: PhoneControlToolJobContext,
        requestedTool: String,
        arguments: JsonObject,
    ): PhoneControlToolExecution = when (handler) {
        PhoneControlHandler.OBSERVE -> handleObserve(job)
        PhoneControlHandler.ACT -> handleAct(job, arguments)
        PhoneControlHandler.DO_STEPS -> handleDoSteps(job, arguments)
        PhoneControlHandler.CLICK_AT -> handleClickAt(job, arguments)
        PhoneControlHandler.ZOOM -> visual.zoom(job, arguments)
        PhoneControlHandler.RESET_VIEW -> visual.resetView(job)
        PhoneControlHandler.SEE_WHOLE_SCREEN -> visual.seeWholeScreen(job)
        PhoneControlHandler.LOOK -> visual.look(job, arguments)
        PhoneControlHandler.CLICK_TARGET -> detector.clickTarget(job, arguments)
        PhoneControlHandler.MAP_TARGETS -> detector.mapTargets(job, arguments)
        PhoneControlHandler.CLICK_MARK -> detector.clickMark(job, arguments)
        PhoneControlHandler.WAIT -> handleWait(job, arguments)
        PhoneControlHandler.TYPE_TEXT -> providers.typeText(job, arguments)
        PhoneControlHandler.SCROLL -> handleScroll(job, arguments)
        PhoneControlHandler.DRAG -> handleDrag(job, arguments)
        PhoneControlHandler.DRAG_TARGET -> detector.dragTarget(job, arguments)
        PhoneControlHandler.KEY_COMBINATION -> providers.keyCombination(job, arguments)
        PhoneControlHandler.OPEN_URL -> providers.openUrl(job, arguments)
        PhoneControlHandler.LAUNCH_APP -> providers.launchApp(job, arguments)
        PhoneControlHandler.SYSTEM_QUERY -> providers.systemQuery(job, arguments)
        PhoneControlHandler.LIST_FILES -> providers.listFiles(job, arguments)
        PhoneControlHandler.READ_TEXT_FILE -> providers.readTextFile(job, arguments)
        PhoneControlHandler.EDIT_TEXT_FILE -> providers.editTextFile(job, arguments)
        PhoneControlHandler.RUN_COMMAND -> handleRunCommand(job, arguments, commands)
        PhoneControlHandler.FOCUS_WINDOW -> surfaces.focusWindow(job, arguments)
        PhoneControlHandler.LIST_WINDOWS -> surfaces.listWindows(job)
        PhoneControlHandler.MINIMIZE_WINDOW -> surfaces.minimizeWindow(job, arguments)
        PhoneControlHandler.RESIZE_WINDOW -> surfaces.unsupportedGeometry(job, "resize_window", arguments)
        PhoneControlHandler.MOVE_WINDOW -> surfaces.unsupportedGeometry(job, "move_window", arguments)
        PhoneControlHandler.READ_CLIPBOARD -> providers.readClipboard(job)
        PhoneControlHandler.ARTIFACT_INFO -> providers.artifactInfo(job, arguments)
        PhoneControlHandler.EXTRACT_ARTIFACT -> providers.extractArtifact(job, arguments)
        PhoneControlHandler.SAVE_ARTIFACT -> providers.saveArtifact(job, arguments)
        PhoneControlHandler.PASTE_ARTIFACT -> providers.pasteArtifact(job, arguments)
        PhoneControlHandler.SEARCH_MEMORY -> memory.searchMemory(job, arguments)
        PhoneControlHandler.OPEN_MEMORY -> memory.openMemory(job, arguments)
        PhoneControlHandler.BROWSER_SETUP -> providers.browserSetup(job)
        PhoneControlHandler.BROWSER_STATUS -> providers.browserStatus(job)
        PhoneControlHandler.BROWSER_READ_PAGE -> providers.browserReadPage(job)
        PhoneControlHandler.BROWSER_EXTRACT_PAGE -> providers.browserExtractPage(job)
        PhoneControlHandler.BROWSER_NAVIGATE -> providers.browserNavigate(job, arguments)
        PhoneControlHandler.BROWSER_HISTORY -> providers.browserHistory(job, arguments)
        PhoneControlHandler.DONE -> handleDone(job, arguments)
    }
}

internal suspend fun handleWait(
    job: PhoneControlToolJobContext,
    args: JsonObject,
): PhoneControlToolExecution {
    val seconds = args.number("seconds")
        ?: return invalidArgs(job, "wait", "wait requires seconds")
    if (!seconds.isFinite() || seconds !in 0.0..MAX_WAIT_SECONDS) {
        return invalidArgs(job, "wait", "seconds must be between 0 and $MAX_WAIT_SECONDS")
    }
    delay((seconds * 1_000.0).roundToLong())
    return PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = "wait",
            capability = "local_completion_and_cleanup",
            provider = "android_app_api",
            providerState = CapabilityState.READY,
            code = "ok",
            observationGeneration = 0,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
        ),
        mutating = false,
    )
}

internal fun handleDone(
    job: PhoneControlToolJobContext,
    args: JsonObject,
): PhoneControlToolExecution {
    val summary = args.string("summary")
        ?: return invalidArgs(job, "done", "done requires summary")
    if (summary.length > MAX_DONE_SUMMARY_CHARS) {
        return invalidArgs(
            job,
            "done",
            "summary exceeds $MAX_DONE_SUMMARY_CHARS characters",
        )
    }
    return PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = "done",
            capability = "local_completion_and_cleanup",
            provider = "android_app_api",
            providerState = CapabilityState.READY,
            code = "ok",
            observationGeneration = 0,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
        ),
        mutating = false,
        terminalSummary = summary,
    )
}

private const val MAX_WAIT_SECONDS = 30.0
private const val MAX_DONE_SUMMARY_CHARS = 320

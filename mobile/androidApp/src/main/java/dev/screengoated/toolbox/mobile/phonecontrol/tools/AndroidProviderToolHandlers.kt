package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlResourceAuthorization
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlResourceAuthorizer
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlStructuralEditAuthorization
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidAppProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidFileProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidSafProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.PhoneControlArtifactStore
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.AndroidBrowserProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.AndroidChromeCdpProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.PublicWebResearchProvider
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import java.io.File
import java.net.URI
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put

internal class AndroidProviderToolHandlers(
    context: Context,
    private val structuralAuthorization: PhoneControlStructuralEditAuthorization =
        PhoneControlStructuralEditAuthorization(context),
    private val resourceAuthorization: PhoneControlResourceAuthorizer =
        PhoneControlResourceAuthorization(context),
) {
    private val artifacts = PhoneControlArtifactStore(context)
    private val app = AndroidAppProvider(context)
    private val files = AndroidFileProvider(artifacts)
    private val structuralEdits = StructuralEditToolCoordinator(
        files,
        structuralAuthorization,
        resourceAuthorization,
    )
    private val saf = AndroidSafProvider(context, artifacts)
    private val artifactHandlers = ArtifactToolHandlers(artifacts, files, resourceAuthorization)
    private val browserHandlers = BrowserToolHandlers(
        provider = AndroidBrowserProvider(context, artifacts),
        deep = AndroidChromeCdpProvider(context, artifacts),
        research = PublicWebResearchProvider(artifacts),
    )
    private val textHandlers = TextToolHandlers(ArtifactStoreTextResolver(artifacts))
    private val surfaceHandlers = SurfaceToolHandlers(context)
    private val systemNavigationHandlers = AndroidSystemNavigationToolHandler(
        AndroidSurfaceToolBackend(context),
    )
    private val fileListings = FileListingToolHandler(context, files, saf)
    private val resourceLauncher = ResourceLaunchToolHandler(context, app)

    suspend fun typeText(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = textHandlers.typeText(job, args)

    suspend fun keyCombination(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val systemKey = parseAndroidSystemNavigationKey(args.string("keys"))
        return if (systemKey == null) {
            textHandlers.keyCombination(job, args)
        } else {
            systemNavigationHandlers.execute(job, args, systemKey)
        }
    }

    suspend fun pasteArtifact(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = textHandlers.pasteArtifact(job, args)

    fun openUrl(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val url = args.string("url") ?: return invalidArgs(
            job = job,
            tool = "open_url",
            message = "open_url requires url",
            argumentField = "url",
            contractReason = "missing_or_invalid",
        )
        return providerResult(
            job,
            "open_url",
            "browser_authenticated_navigation",
            "android_app_api",
            mutating = true,
            invalidatesSnapshot = true,
            result = app.openUrl(url),
        )
    }

    suspend fun launchApp(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val name = args.string("name")
            ?: return invalidArgs(
                job,
                "launch_app",
                "launch_app requires name",
                argumentField = "name",
                contractReason = "missing_or_invalid",
            )
        return resourceLauncher.execute(job, name, args.string("args"))
    }

    suspend fun systemQuery(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val domain = args.string("domain")
            ?: return invalidArgs(
                job,
                "system_query",
                "system_query requires domain",
                argumentField = "domain",
                contractReason = "missing_or_invalid",
            )
        val query = args.string("query")
            ?: return invalidArgs(
                job,
                "system_query",
                "system_query requires query",
                argumentField = "query",
                contractReason = "missing_or_invalid",
            )
        if (!isSupportedSystemQuery(domain, query)) {
            return invalidArgs(
                job,
                "system_query",
                "Unsupported system_query domain/query pair.",
                argumentField = "domain_query",
                contractReason = "unsupported_pair",
            )
        }
        val filters = args["args"] as? JsonObject
        if (filters != null && filters.isNotEmpty()) {
            return unavailableToolResponse(
                job,
                "system_query",
                "system_query",
                "android_app_api",
                CapabilityState.UNSUPPORTED,
            )
        }
        if (domain == "window") return surfaceHandlers.queryWindows(job, query)
        val provider = if (domain == "clipboard") {
            "accessibility"
        } else {
            "android_app_api"
        }
        return providerResult(
            job,
            "system_query",
            "system_query",
            provider,
            mutating = false,
            result = app.systemQuery(domain, query),
        )
    }

    suspend fun readClipboard(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        providerResult(
            job,
            "read_clipboard",
            "system_query",
            "accessibility",
            mutating = false,
            result = app.systemQuery("clipboard", "text"),
        )

    suspend fun listFiles(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = fileListings.execute(job, args)

    fun readTextFile(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val path = args.string("path")
            ?: return invalidArgs(job, "read_text_file", "read_text_file requires path")
        val maxChars = args.int("max_chars") ?: MAX_TEXT_CHARS
        if (maxChars !in 1..MAX_TEXT_CHARS) {
            return invalidArgs(job, "read_text_file", "max_chars must be 1 to $MAX_TEXT_CHARS")
        }
        val result = if (isContentUri(path)) {
            saf.readText(path, args.string("expected_sha256"), maxChars)
        } else {
            if (!isAbsolutePath(path)) return unavailableStoragePath(job, "read_text_file")
            files.readText(path, args.string("expected_sha256"), maxChars)
        }
        return providerResult(
            job,
            "read_text_file",
            "file_resource_access",
            "android_app_api",
            mutating = false,
            result = result,
        )
    }

    suspend fun editTextFile(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val request = when (val parsed = parseExactEditArguments(job, args, "edit_text_file")) {
            is ExactEditArguments.Invalid -> return parsed.response
            is ExactEditArguments.Valid -> parsed.request
        }
        val result = executeResourceScopedMutation(
            tool = "edit_text_file",
            arguments = args,
            authorizer = resourceAuthorization,
        ) { targetLease ->
            files.exactReplace(
                request.path,
                request.expectedSha256,
                request.replacements,
                targetLease,
            )
        }
        return providerResult(
            job,
            "edit_text_file",
            "file_resource_access",
            "android_app_api",
            mutating = true,
            result = result,
        )
    }

    suspend fun editTextFileStructure(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val request = when (
            val parsed = parseExactEditArguments(job, args, "edit_text_file_structure")
        ) {
            is ExactEditArguments.Invalid -> return parsed.response
            is ExactEditArguments.Valid -> parsed.request
        }
        return providerResult(
            job,
            "edit_text_file_structure",
            "file_resource_access",
            "android_app_api",
            mutating = true,
            result = structuralEdits.execute(args, request),
        )
    }

    fun artifactInfo(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = artifactHandlers.info(job, args)

    fun extractArtifact(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = artifactHandlers.extract(job, args)

    suspend fun saveArtifact(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = artifactHandlers.save(job, args)

    suspend fun browserSetup(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        browserHandlers.setup(job)

    suspend fun browserStatus(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        browserHandlers.status(job)

    suspend fun browserReset(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        browserHandlers.reset(job)

    suspend fun browserReadPage(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        browserHandlers.readPage(job)

    suspend fun researchWeb(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = browserHandlers.research(job, args)

    suspend fun browserExtractPage(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        browserHandlers.extractPage(job)

    suspend fun browserWaitFor(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = browserHandlers.waitFor(job, args)

    suspend fun browserEval(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = browserHandlers.eval(job, args)

    suspend fun browserNavigate(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = browserHandlers.navigate(job, args)

    suspend fun browserHistory(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = browserHandlers.history(job, args)

    suspend fun browserOpenTab(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = browserHandlers.openTab(job, args)

    suspend fun browserUpload(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = browserHandlers.upload(job, args)

    suspend fun browserTabs(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        browserHandlers.tabs(job)

    suspend fun browserSwitchTab(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = browserHandlers.switchTab(job, args)

    suspend fun browserCloseTab(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = browserHandlers.closeTab(job, args)

    suspend fun browserNetwork(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = browserHandlers.network(job, args)

    suspend fun browserConsole(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        browserHandlers.console(job)

}

internal fun providerResult(
    job: PhoneControlToolJobContext,
    requestedTool: String,
    capability: String,
    provider: String,
    mutating: Boolean,
    result: AndroidProviderResult,
    invalidatesSnapshot: Boolean = false,
): PhoneControlToolExecution = when (result) {
    is AndroidProviderResult.Success -> {
        val effect = EffectCertainty.fromSignals(
            effectVerified = result.effectVerified,
            effectMayHaveOccurred = result.effectMayHaveOccurred,
        )
        PhoneControlToolExecution(
            response = toolResponse(
                job = job,
                requestedTool = requestedTool,
                capability = capability,
                provider = provider,
                providerState = CapabilityState.READY,
                code = "ok",
                observationGeneration = result.data["observation_generation"]
                    ?.jsonPrimitive
                    ?.longOrNull
                    ?: 0,
                effect = effect,
                snapshotInvalidated = invalidatesSnapshot && effect.effectMayHaveOccurred == true,
                freshObservationRequired = (invalidatesSnapshot && effect.effectMayHaveOccurred == true)
                    .takeIf { it },
                data = result.data,
            ),
            mutating = effect.effectMayHaveOccurred == true,
            refreshScreenFrame = invalidatesSnapshot && effect.effectMayHaveOccurred == true,
        )
    }
    is AndroidProviderResult.Failure -> {
        val unavailable = result.code == "capability_unavailable"
        val effectMayHaveOccurred = mutating && result.code in UNCERTAIN_MUTATION_FAILURES
        PhoneControlToolExecution(
            response = toolResponse(
                job = job,
                requestedTool = requestedTool,
                capability = capability,
                provider = provider,
                providerState = when {
                    !unavailable -> CapabilityState.READY
                    result.requiredUserStep != null -> CapabilityState.NEEDS_USER_STEP
                    else -> CapabilityState.UNAVAILABLE
                },
                code = result.code,
                observationGeneration = 0,
                effect = if (effectMayHaveOccurred) {
                    EffectCertainty.MAY_HAVE_OCCURRED
                } else {
                    EffectCertainty.PROVEN_NO_EFFECT
                },
                snapshotInvalidated = effectMayHaveOccurred,
                retryable = result.retryable,
                requiredUserStep = result.requiredUserStep,
                data = buildJsonObject {
                    result.data.forEach { (key, value) -> put(key, value) }
                    put("message", result.message)
                },
            ),
            mutating = effectMayHaveOccurred,
            refreshScreenFrame = effectMayHaveOccurred,
        )
    }
}

private fun unavailableStoragePath(
    job: PhoneControlToolJobContext,
    tool: String,
): PhoneControlToolExecution = unavailableToolResponse(
    job,
    tool,
    "file_resource_access",
    "android_app_api",
    CapabilityState.NEEDS_USER_STEP,
    "grant_storage_access",
)

private fun isAbsolutePath(path: String): Boolean = runCatching { File(path.trim()).isAbsolute }
    .getOrDefault(false)

private fun isContentUri(path: String): Boolean = runCatching {
    URI(path.trim()).scheme.equals("content", ignoreCase = true)
}.getOrDefault(false)

internal fun isSupportedSystemQuery(domain: String, query: String): Boolean =
    SUPPORTED_SYSTEM_QUERIES[domain] == query

private const val MAX_TEXT_CHARS = 64_000
private val UNCERTAIN_MUTATION_FAILURES = setOf("write_failed", "save_failed")
private val SUPPORTED_SYSTEM_QUERIES = mapOf(
    "capabilities" to "list",
    "audio" to "active_sessions",
    "clipboard" to "text",
    "process" to "list_basic",
    "storage" to "volumes",
    "window" to "list",
)

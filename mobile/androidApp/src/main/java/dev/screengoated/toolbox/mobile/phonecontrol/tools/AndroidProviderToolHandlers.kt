package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidAppProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidFileProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidSafProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.ExactReplacement
import dev.screengoated.toolbox.mobile.phonecontrol.provider.PhoneControlArtifactStore
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.AndroidBrowserProvider
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import java.io.File
import java.net.URI
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put

internal class AndroidProviderToolHandlers(
    context: Context,
) {
    private val artifacts = PhoneControlArtifactStore(context)
    private val app = AndroidAppProvider(context)
    private val files = AndroidFileProvider(artifacts)
    private val saf = AndroidSafProvider(context, artifacts)
    private val artifactHandlers = ArtifactToolHandlers(artifacts, files)
    private val browserHandlers = BrowserToolHandlers(AndroidBrowserProvider(context, artifacts))
    private val textHandlers = TextToolHandlers(ArtifactStoreTextResolver(artifacts))
    private val surfaceHandlers = SurfaceToolHandlers(context)
    private val systemNavigationHandlers = AndroidSystemNavigationToolHandler(
        AndroidSurfaceToolBackend(context),
    )

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
        val url = args.string("url") ?: return invalidArgs(job, "open_url", "open_url requires url")
        val scheme = runCatching { URI(url.trim()).scheme?.lowercase() }.getOrNull()
        if (scheme !in setOf("http", "https")) {
            return unavailableToolResponse(
                job,
                "open_url",
                "browser_authenticated_navigation",
                "android_app_api",
                CapabilityState.UNSUPPORTED,
            )
        }
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

    fun launchApp(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val name = args.string("name")
            ?: return invalidArgs(job, "launch_app", "launch_app requires name")
        if (!args.string("args").isNullOrBlank()) {
            return unavailableToolResponse(
                job,
                "launch_app",
                "app_and_task_control",
                "android_app_api",
                CapabilityState.UNSUPPORTED,
            )
        }
        return providerResult(
            job,
            "launch_app",
            "app_and_task_control",
            "android_app_api",
            mutating = true,
            invalidatesSnapshot = true,
            result = app.launchApp(name),
        )
    }

    suspend fun systemQuery(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val domain = args.string("domain")
            ?: return invalidArgs(job, "system_query", "system_query requires domain")
        val query = args.string("query")
            ?: return invalidArgs(job, "system_query", "system_query requires query")
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

    fun listFiles(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val path = args.string("path")
            ?: return invalidArgs(job, "list_files", "list_files requires path")
        val kind = args.string("kind")
        if (kind != null && kind !in setOf("any", "file", "directory")) {
            return invalidArgs(job, "list_files", "kind is not supported")
        }
        val extensions = args.stringSet("extensions")
            ?: return invalidArgs(job, "list_files", "extensions must contain only strings")
        val sortBy = args.string("sort_by") ?: "modified"
        if (sortBy !in setOf("modified", "created", "name", "size")) {
            return invalidArgs(job, "list_files", "sort_by is not supported")
        }
        val order = args.string("order") ?: "descending"
        if (order !in setOf("descending", "ascending")) {
            return invalidArgs(job, "list_files", "order is not supported")
        }
        val limit = args.int("limit") ?: DEFAULT_FILE_LIMIT
        if (limit <= 0) return invalidArgs(job, "list_files", "limit must be positive")
        if (isContentUri(path)) {
            if (kind !in setOf(null, "any") || extensions.isNotEmpty() || sortBy == "created") {
                return unsupportedSafVariant(job, "list_files")
            }
            return providerResult(
                job,
                "list_files",
                "file_resource_access",
                "android_app_api",
                mutating = false,
                result = saf.list(path, sortBy, order == "descending", limit),
            )
        }
        if (!isAbsolutePath(path)) return unavailableStoragePath(job, "list_files")
        return providerResult(
            job,
            "list_files",
            "file_resource_access",
            "android_app_api",
            mutating = false,
            result = files.list(path, kind, extensions, sortBy, order == "descending", limit),
        )
    }

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

    fun editTextFile(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val path = args.string("path")
            ?: return invalidArgs(job, "edit_text_file", "edit_text_file requires path")
        if (isContentUri(path)) return unsupportedSafVariant(job, "edit_text_file")
        if (!isAbsolutePath(path)) return unavailableStoragePath(job, "edit_text_file")
        val expectedSha = args.string("expected_sha256")?.takeIf(String::isNotBlank)
            ?: return invalidArgs(job, "edit_text_file", "expected_sha256 is required")
        val rawReplacements = args["replacements"] as? JsonArray
            ?: return invalidArgs(job, "edit_text_file", "replacements must be an array")
        if (rawReplacements.size !in 1..MAX_REPLACEMENTS) {
            return invalidArgs(
                job,
                "edit_text_file",
                "replacements must contain 1 to $MAX_REPLACEMENTS items",
            )
        }
        val replacements = rawReplacements.mapIndexed { index, element ->
            val replacement = element as? JsonObject
                ?: return invalidArgs(job, "edit_text_file", "replacement ${index + 1} is not an object")
            val oldText = replacement.string("old_text")?.takeIf(String::isNotEmpty)
                ?: return invalidArgs(job, "edit_text_file", "replacement ${index + 1} needs old_text")
            val newText = replacement.string("new_text")
                ?: return invalidArgs(job, "edit_text_file", "replacement ${index + 1} needs new_text")
            val expectedCount = replacement.int("expected_count")?.takeIf { it > 0 }
                ?: return invalidArgs(
                    job,
                    "edit_text_file",
                    "replacement ${index + 1} needs positive expected_count",
                )
            ExactReplacement(oldText, newText, expectedCount)
        }
        return providerResult(
            job,
            "edit_text_file",
            "file_resource_access",
            "android_app_api",
            mutating = true,
            result = files.exactReplace(path, expectedSha, replacements),
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

    fun saveArtifact(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = artifactHandlers.save(job, args)

    suspend fun browserSetup(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        browserHandlers.setup(job)

    suspend fun browserStatus(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        browserHandlers.status(job)

    suspend fun browserReadPage(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        browserHandlers.readPage(job)

    suspend fun browserExtractPage(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        browserHandlers.extractPage(job)

    suspend fun browserNavigate(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = browserHandlers.navigate(job, args)

    suspend fun browserHistory(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution = browserHandlers.history(job, args)
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
                data = buildJsonObject { put("message", result.message) },
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

private fun unsupportedSafVariant(
    job: PhoneControlToolJobContext,
    tool: String,
): PhoneControlToolExecution = unavailableToolResponse(
    job,
    tool,
    "file_resource_access",
    "android_app_api",
    CapabilityState.UNSUPPORTED,
)

private fun JsonObject.stringSet(name: String): Set<String>? {
    val value = get(name) ?: return emptySet()
    val array = value as? JsonArray ?: return null
    val strings = array.map { element ->
        (element as? JsonPrimitive)?.takeIf(JsonPrimitive::isString)?.contentOrNull ?: return null
    }
    return strings.toSet()
}

private fun isAbsolutePath(path: String): Boolean = runCatching { File(path.trim()).isAbsolute }
    .getOrDefault(false)

private fun isContentUri(path: String): Boolean = runCatching {
    URI(path.trim()).scheme.equals("content", ignoreCase = true)
}.getOrDefault(false)

private const val DEFAULT_FILE_LIMIT = 200
private const val MAX_TEXT_CHARS = 64_000
private const val MAX_REPLACEMENTS = 64
private val UNCERTAIN_MUTATION_FAILURES = setOf("write_failed", "save_failed")

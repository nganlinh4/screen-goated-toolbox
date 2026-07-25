package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import android.os.Build
import android.os.Environment
import android.os.storage.StorageManager
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.capability.PhoneControlProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidFileProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidSafProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandResult
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import java.io.File
import java.net.URI
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.util.Base64
import java.util.Locale
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

internal class FileListingToolHandler(
    private val context: Context,
    private val files: AndroidFileProvider,
    private val saf: AndroidSafProvider,
    private val sharedStorageRoot: File = primarySharedStorageRoot(context),
) {
    suspend fun execute(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val request = parseRequest(args) ?: return invalidFileListRequest(
            job,
            args,
            FILE_KINDS,
            SORT_FIELDS,
            ORDERS,
            MAX_LIMIT,
        )
        if (isContentUri(request.path)) return listSaf(job, request)
        val resolution = resolveAndroidFileListPath(request.path, sharedStorageRoot)
        if (resolution !is AndroidFileListPathResolution.Resolved) {
            return invalidArgs(
                job,
                TOOL,
                "path must be absolute or a supported standard folder name",
                argumentField = "path",
                contractReason = "unsupported_path_form",
            )
        }

        val path = resolution.file.absolutePath
        val ordinary = files.list(
            path,
            request.kind,
            request.extensions,
            request.sortBy,
            request.descending,
            request.limit,
        )
        val ordinaryScopeComplete = isAppOwnedPath(resolution.file)
        if (ordinaryScopeComplete) {
            return providerResult(
                job,
                TOOL,
                CAPABILITY,
                APP_PROVIDER,
                mutating = false,
                result = ordinary.withCoverage(complete = true),
            )
        }

        val elevated = selectedElevatedProvider()
        if (elevated == null) {
            return providerResult(
                job,
                TOOL,
                CAPABILITY,
                APP_PROVIDER,
                mutating = false,
                result = ordinary.withCoverage(complete = false),
            )
        }
        val probe = elevated.awaitReady(context)
        if (probe.state != CapabilityState.READY) {
            return elevatedUnavailable(job, elevated.providerId, probe.state, probe.requiredUserStep)
        }
        return listElevated(job, elevated, resolution.file, request)
    }

    private fun parseRequest(args: JsonObject): FileListRequest? {
        val path = args.string("path")?.trim()?.takeIf(String::isNotEmpty) ?: return null
        val kind = args.string("kind")
        if (kind != null && kind !in FILE_KINDS) return null
        if ("kind" in args && kind == null) return null
        val extensions = args.stringSet("extensions") ?: return null
        val sortBy = args.string("sort_by") ?: DEFAULT_SORT
        if (sortBy !in SORT_FIELDS || ("sort_by" in args && args.string("sort_by") == null)) {
            return null
        }
        val order = args.string("order") ?: DEFAULT_ORDER
        if (order !in ORDERS || ("order" in args && args.string("order") == null)) return null
        val limit = args.intValue("limit") ?: if ("limit" in args) return null else DEFAULT_LIMIT
        if (limit !in 1..MAX_LIMIT) return null
        return FileListRequest(
            path = path,
            kind = kind,
            extensions = extensions,
            sortBy = sortBy,
            descending = order == "descending",
            limit = limit,
        )
    }

    private fun listSaf(
        job: PhoneControlToolJobContext,
        request: FileListRequest,
    ): PhoneControlToolExecution {
        if (
            request.kind !in setOf(null, "any") ||
            request.extensions.isNotEmpty() ||
            request.sortBy == "created"
        ) {
            return unavailableToolResponse(
                job,
                TOOL,
                CAPABILITY,
                APP_PROVIDER,
                CapabilityState.UNSUPPORTED,
            )
        }
        return providerResult(
            job,
            TOOL,
            CAPABILITY,
            APP_PROVIDER,
            mutating = false,
            result = saf.list(
                request.path,
                request.sortBy,
                request.descending,
                request.limit,
            ),
        )
    }

    private suspend fun listElevated(
        job: PhoneControlToolJobContext,
        provider: PrivilegedCommandProvider,
        directory: File,
        request: FileListRequest,
    ): PhoneControlToolExecution {
        val result = provider.executeAuthorized(
            context = context,
            effectOwner = job.effectOwner,
            program = SHELL,
            args = listOf("-c", READ_ONLY_LIST_SCRIPT, SCRIPT_NAME, directory.absolutePath),
            cwd = ROOT_DIRECTORY,
            timeoutMs = LIST_TIMEOUT_MS,
            effectMayChangeUserState = false,
        )
        currentCoroutineContext().ensureActive()
        return when (result) {
            is PrivilegedCommandResult.Failure ->
                privilegedFailure(job, provider.providerId, result)
            is PrivilegedCommandResult.Success ->
                privilegedReceipt(job, provider.providerId, directory, request, 0, result)
        }
    }

    private fun privilegedReceipt(
        job: PhoneControlToolJobContext,
        providerId: String,
        directory: File,
        request: FileListRequest,
        observationGeneration: Long,
        result: PrivilegedCommandResult.Success,
    ): PhoneControlToolExecution {
        val receipt = result.receipt
        if (receipt["output_truncated"]?.jsonPrimitive?.booleanOrNull == true) {
            return elevatedFailure(
                job,
                providerId,
                "listing_too_large",
                "The complete directory metadata exceeded the bounded provider output.",
                observationGeneration,
                retryable = false,
            )
        }
        val code = receipt["code"]?.jsonPrimitive?.contentOrNull
        val exitCode = receipt["exit_code"]?.jsonPrimitive?.intOrNull
        if (code != "process_exited" || exitCode == null) {
            return elevatedFailure(
                job,
                providerId,
                code ?: "provider_transport_failure",
                "The selected provider did not return a terminal listing receipt.",
                observationGeneration,
                retryable = true,
            )
        }
        if (exitCode == PATH_UNAVAILABLE_EXIT) {
            return elevatedFailure(
                job,
                providerId,
                "path_unavailable",
                "The directory is unavailable to the selected provider.",
                observationGeneration,
                retryable = false,
            )
        }
        if (exitCode != 0) {
            return elevatedFailure(
                job,
                providerId,
                "listing_failed",
                "The selected provider could not complete the read-only listing.",
                observationGeneration,
                retryable = true,
            )
        }
        val output = receipt["output"]?.jsonPrimitive?.contentOrNull.orEmpty()
        val entries = when (val parsed = parsePrivilegedFileListing(output, directory)) {
            is PrivilegedFileListingParseResult.Failure -> {
                return elevatedFailure(
                    job,
                    providerId,
                    parsed.code,
                    parsed.message,
                    observationGeneration,
                    retryable = false,
                )
            }
            is PrivilegedFileListingParseResult.Success -> parsed.entries
        }
        val data = fileListingData(directory, entries, request)
        return PhoneControlToolExecution(
            response = toolResponse(
                job = job,
                requestedTool = TOOL,
                capability = CAPABILITY,
                provider = providerId,
                providerState = CapabilityState.READY,
                code = "ok",
                observationGeneration = observationGeneration,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                data = data,
            ),
            mutating = false,
        )
    }

    private fun selectedElevatedProvider(): PrivilegedCommandProvider? {
        val selected = PhoneControlPowerPreferences.current(context)?.elevatedProviderId ?: return null
        if (selected !in PhoneControlProviderRegistry.providersFor(context, CAPABILITY)) return null
        return PrivilegedCommandProviderRegistry.find(selected)
    }

    private fun isAppOwnedPath(path: File): Boolean {
        val roots = buildList {
            add(context.filesDir)
            add(context.cacheDir)
            add(context.noBackupFilesDir)
            add(context.codeCacheDir)
            context.externalCacheDirs.filterNotNull().forEach(::add)
            context.getExternalFilesDirs(null).filterNotNull().forEach(::add)
        }
        return roots.any { root -> path.isWithin(root) }
    }
}

internal sealed interface AndroidFileListPathResolution {
    data class Resolved(val file: File, val standardFolder: String?) :
        AndroidFileListPathResolution

    data object Invalid : AndroidFileListPathResolution
}

internal fun resolveAndroidFileListPath(
    rawPath: String,
    sharedStorageRoot: File,
): AndroidFileListPathResolution {
    val value = rawPath.trim()
    val standard = value.lowercase(Locale.ROOT).takeIf(STANDARD_FOLDERS::containsKey)
    val unresolved = standard?.let { File(sharedStorageRoot, STANDARD_FOLDERS.getValue(it)) }
        ?: File(value).takeIf(File::isAbsolute)
        ?: return AndroidFileListPathResolution.Invalid
    val resolved = runCatching { unresolved.canonicalFile }.getOrNull()
        ?: return AndroidFileListPathResolution.Invalid
    return AndroidFileListPathResolution.Resolved(resolved, standard)
}

internal data class PrivilegedFileEntry(
    val path: String,
    val name: String,
    val kind: String,
    val size: Long,
    val modifiedMs: Long,
)

internal sealed interface PrivilegedFileListingParseResult {
    data class Success(val entries: List<PrivilegedFileEntry>) :
        PrivilegedFileListingParseResult

    data class Failure(val code: String, val message: String) :
        PrivilegedFileListingParseResult
}

internal fun parsePrivilegedFileListing(
    output: String,
    directory: File,
): PrivilegedFileListingParseResult {
    if (output.isEmpty()) return PrivilegedFileListingParseResult.Success(emptyList())
    val base = runCatching { directory.canonicalFile }.getOrNull()
        ?: return malformedListing()
    val entries = mutableListOf<PrivilegedFileEntry>()
    for (line in output.lineSequence()) {
        if (line.isEmpty()) continue
        val fields = line.split('\t')
        if (fields.size != LIST_FIELD_COUNT) return malformedListing()
        val kind = when (fields[0]) {
            "d" -> "directory"
            "f" -> "file"
            else -> return malformedListing()
        }
        val size = fields[1].toLongOrNull()?.takeIf { it >= 0 } ?: return malformedListing()
        val modifiedSeconds = fields[2].toLongOrNull()?.takeIf { it >= 0 }
            ?: return malformedListing()
        val decodedPath = decodeBase64Utf8(fields[3]) ?: return malformedListing()
        val file = File(decodedPath).absoluteFile
        val parent = runCatching { file.parentFile?.canonicalFile }.getOrNull()
            ?: return malformedListing()
        val name = file.name
        if (parent != base || name.isEmpty() || name == "." || name == "..") {
            return malformedListing()
        }
        val normalized = File(base, name).absoluteFile
        entries += PrivilegedFileEntry(
            path = normalized.absolutePath,
            name = normalized.name,
            kind = kind,
            size = size,
            modifiedMs = modifiedSeconds.coerceAtMost(Long.MAX_VALUE / 1_000L) * 1_000L,
        )
    }
    return PrivilegedFileListingParseResult.Success(entries)
}

private fun fileListingData(
    directory: File,
    entries: List<PrivilegedFileEntry>,
    request: FileListRequest,
): JsonObject {
    val normalizedExtensions = request.extensions
        .map { it.trim().trimStart('.').lowercase(Locale.ROOT) }
        .toSet()
    val matching = entries.filter { entry ->
        when (request.kind) {
            "file" -> entry.kind == "file"
            "directory" -> entry.kind == "directory"
            else -> true
        }
    }.filter { entry ->
        normalizedExtensions.isEmpty() ||
            entry.kind == "directory" ||
            File(entry.name).extension.lowercase(Locale.ROOT) in normalizedExtensions
    }
    val comparator = when (request.sortBy) {
        "name" -> compareBy<PrivilegedFileEntry> { it.name.lowercase(Locale.ROOT) }
        "size" -> compareBy(PrivilegedFileEntry::size)
        "created", "modified" -> compareBy(PrivilegedFileEntry::modifiedMs)
        else -> error("validated sort field changed")
    }.thenBy { it.path }
    val sorted = matching.sortedWith(if (request.descending) comparator.reversed() else comparator)
    val returned = sorted.take(request.limit)
    return buildJsonObject {
        put("path", directory.absolutePath)
        put("count", returned.size)
        put("total_entry_count", entries.size)
        put("matched_entry_count", matching.size)
        put("listing_complete", matching.size <= request.limit)
        put("coverage", "provider_complete")
        put("items", buildJsonArray {
            returned.forEach { entry ->
                add(buildJsonObject {
                    put("name", entry.name)
                    put("path", entry.path)
                    put("kind", entry.kind)
                    put("size", entry.size)
                    put("modified_ms", entry.modifiedMs)
                })
            }
        })
    }
}

private fun AndroidProviderResult.withCoverage(complete: Boolean): AndroidProviderResult = when (this) {
    is AndroidProviderResult.Failure -> this
    is AndroidProviderResult.Success -> copy(
        data = JsonObject(
            data + buildJsonObject {
                put("listing_complete", complete && data.booleanValue("listing_complete") != false)
                put("coverage", if (complete) "app_owned_complete" else "app_visible_only")
            },
        ),
    )
}

private fun elevatedUnavailable(
    job: PhoneControlToolJobContext,
    providerId: String,
    state: CapabilityState,
    requiredUserStep: String?,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = TOOL,
        capability = CAPABILITY,
        provider = providerId,
        providerState = state,
        code = "capability_unavailable",
        observationGeneration = PhoneControlAccessibilityProvider.observationGeneration,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = state != CapabilityState.UNSUPPORTED,
        requiredUserStep = requiredUserStep,
        data = buildJsonObject {
            put("message", "The selected authority is not ready for a complete filesystem listing.")
        },
    ),
    mutating = false,
)

private fun privilegedFailure(
    job: PhoneControlToolJobContext,
    providerId: String,
    failure: PrivilegedCommandResult.Failure,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = TOOL,
        capability = CAPABILITY,
        provider = providerId,
        providerState = failure.state,
        code = failure.code,
        observationGeneration = PhoneControlAccessibilityProvider.observationGeneration,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = failure.state != CapabilityState.UNSUPPORTED,
        requiredUserStep = failure.requiredUserStep,
        freshObservationRequired = failure.freshObservationRequired,
        data = buildJsonObject {
            put("message", failure.message)
            failure.providerGuidance?.let { put("provider_guidance", it) }
        },
    ),
    mutating = false,
)

private fun elevatedFailure(
    job: PhoneControlToolJobContext,
    providerId: String,
    code: String,
    message: String,
    observationGeneration: Long,
    retryable: Boolean,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = TOOL,
        capability = CAPABILITY,
        provider = providerId,
        providerState = CapabilityState.READY,
        code = code,
        observationGeneration = observationGeneration,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        retryable = retryable,
        data = buildJsonObject { put("message", message) },
    ),
    mutating = false,
)

private fun malformedListing() = PrivilegedFileListingParseResult.Failure(
    code = "provider_contract_failure",
    message = "The selected provider returned malformed directory metadata.",
)

private fun decodeBase64Utf8(value: String): String? = runCatching {
    val bytes = Base64.getDecoder().decode(value)
    Charsets.UTF_8.newDecoder()
        .onMalformedInput(CodingErrorAction.REPORT)
        .onUnmappableCharacter(CodingErrorAction.REPORT)
        .decode(ByteBuffer.wrap(bytes))
        .toString()
}.getOrNull()

private fun JsonObject.stringSet(name: String): Set<String>? {
    val value = get(name) ?: return emptySet()
    val array = value as? JsonArray ?: return null
    return array.map { element ->
        (element as? JsonPrimitive)?.takeIf(JsonPrimitive::isString)?.contentOrNull
            ?: return null
    }.toSet()
}

private fun JsonObject.booleanValue(name: String): Boolean? =
    (get(name) as? JsonPrimitive)?.booleanOrNull

private fun JsonObject.intValue(name: String): Int? =
    (get(name) as? JsonPrimitive)?.intOrNull

private fun File.isWithin(root: File): Boolean = runCatching {
    val candidatePath = canonicalFile.toPath()
    val rootPath = root.canonicalFile.toPath()
    candidatePath == rootPath || candidatePath.startsWith(rootPath)
}.getOrDefault(false)

private fun isContentUri(path: String): Boolean = runCatching {
    URI(path.trim()).scheme.equals("content", ignoreCase = true)
}.getOrDefault(false)

@Suppress("DEPRECATION")
private fun primarySharedStorageRoot(context: Context): File =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        context.getSystemService(StorageManager::class.java)
            ?.primaryStorageVolume
            ?.directory
            ?: Environment.getExternalStorageDirectory()
    } else {
        Environment.getExternalStorageDirectory()
    }

private data class FileListRequest(
    val path: String,
    val kind: String?,
    val extensions: Set<String>,
    val sortBy: String,
    val descending: Boolean,
    val limit: Int,
)

private val STANDARD_FOLDERS = mapOf(
    "home" to "",
    "documents" to "Documents",
    "downloads" to "Download",
    "music" to "Music",
    "pictures" to "Pictures",
    "videos" to "Movies",
)
private val FILE_KINDS = setOf("any", "file", "directory")
private val SORT_FIELDS = setOf("modified", "created", "name", "size")
private val ORDERS = setOf("descending", "ascending")
private const val TOOL = "list_files"
private const val CAPABILITY = "file_resource_access"
private const val APP_PROVIDER = "android_app_api"
private const val DEFAULT_SORT = "modified"
private const val DEFAULT_ORDER = "descending"
private const val DEFAULT_LIMIT = 200
private const val MAX_LIMIT = 200
private const val LIST_TIMEOUT_MS = 15_000L
private const val PATH_UNAVAILABLE_EXIT = 44
private const val LIST_FIELD_COUNT = 4
private const val SHELL = "/system/bin/sh"
private const val ROOT_DIRECTORY = "/"
private const val SCRIPT_NAME = "sgt-list-files"

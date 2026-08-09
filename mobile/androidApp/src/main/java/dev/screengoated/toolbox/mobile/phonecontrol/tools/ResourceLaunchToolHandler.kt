package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.storage.StorageManager
import android.provider.DocumentsContract
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.capability.PhoneControlProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidAppPackageResolution
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidAppProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.PrivilegedCommandResult
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import java.io.File
import java.net.URI
import java.util.Locale
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

internal class ResourceLaunchToolHandler(
    context: Context,
    private val app: AndroidAppProvider,
) {
    private val context = context.applicationContext
    private val sharedStorageRoot = primarySharedStorageRoot(context)

    suspend fun execute(
        job: PhoneControlToolJobContext,
        rawName: String,
        rawArgs: String? = null,
    ): PhoneControlToolExecution {
        val target = classifyResourceLaunchInput(rawName, sharedStorageRoot)
        val resourceArg = rawArgs?.trim()?.takeIf(String::isNotEmpty)
        if (resourceArg == null) return executeTarget(job, target)
        val appTarget = target as? ResourceLaunchInput.App
            ?: return invalidFailure(
                job,
                "args",
                "incompatible_fields",
                "A separate resource argument requires an app name or package in name.",
            )
        val packageName = when (val resolved = app.resolveExactLaunchablePackage(appTarget.name)) {
            is AndroidAppPackageResolution.Failure -> {
                return failure(
                    job,
                    APP_PROVIDER,
                    CapabilityState.READY,
                    resolved.code,
                    resolved.message,
                )
            }
            is AndroidAppPackageResolution.Resolved -> resolved.packageName
        }
        return when (
            val resource = classifyResourceLaunchInput(resourceArg, sharedStorageRoot)
        ) {
            is ResourceLaunchInput.Content -> openContent(job, resource.uri, packageName)
            is ResourceLaunchInput.Path -> openPath(job, resource.file, packageName)
            is ResourceLaunchInput.App -> invalidFailure(
                job,
                "args",
                "unsupported_command_line",
                "Android launch arguments must identify a local file, folder, or content URI.",
                code = "unsupported_arguments",
            )
            is ResourceLaunchInput.Invalid -> failure(
                job,
                APP_PROVIDER,
                CapabilityState.READY,
                resource.code,
                resource.message,
            )
        }
    }

    private suspend fun executeTarget(
        job: PhoneControlToolJobContext,
        input: ResourceLaunchInput,
    ): PhoneControlToolExecution = when (input) {
        is ResourceLaunchInput.App -> launchApp(job, input.name)
        is ResourceLaunchInput.Content -> openContent(job, input.uri, null)
        is ResourceLaunchInput.Path -> openPath(job, input.file, null)
        is ResourceLaunchInput.Invalid -> failure(
            job = job,
            providerId = APP_PROVIDER,
            state = CapabilityState.READY,
            code = input.code,
            message = input.message,
        )
    }

    private suspend fun launchApp(
        job: PhoneControlToolJobContext,
        name: String,
    ): PhoneControlToolExecution {
        val packageName = when (val resolved = app.resolveExactLaunchablePackage(name)) {
            is AndroidAppPackageResolution.Failure -> {
                return appResult(
                    job,
                    AndroidProviderResult.Failure(resolved.code, resolved.message),
                )
            }
            is AndroidAppPackageResolution.Resolved -> resolved.packageName
        }
        val observation = if (!PhoneControlAccessibilityProvider.isReady) {
            null
        } else {
            when (val observed = PhoneControlAccessibilityProvider.observe()) {
                is AccessibilityProviderResult.Success -> observed.value
                is AccessibilityProviderResult.Failure -> null
            }
        }
        if (observation != null && shouldPreserveForegroundLaunch(packageName, observation)) {
            return appResult(
                job,
                AndroidProviderResult.Success(
                    buildJsonObject {
                        put("package", packageName)
                        put("launch_disposition", "preserved_foreground")
                        put("observation_generation", observation.generation)
                    },
                ),
            )
        }
        return appResult(job, app.launchApp(packageName))
    }

    private fun openContent(
        job: PhoneControlToolJobContext,
        uri: Uri,
        packageName: String?,
    ): PhoneControlToolExecution {
        val mime = context.contentResolver.getType(uri) ?: BINARY_MIME
        return appResult(job, app.openResource(uri, mime, "content_uri", packageName))
    }

    private suspend fun openPath(
        job: PhoneControlToolJobContext,
        file: File,
        packageName: String?,
    ): PhoneControlToolExecution {
        val canonical = runCatching { file.canonicalFile }.getOrElse {
            return failure(
                job,
                APP_PROVIDER,
                CapabilityState.READY,
                "invalid_arguments",
                "The resource path could not be normalized.",
            )
        }
        if (canonical.exists()) {
            val ordinaryUri = ordinaryResourceUri(canonical)
            if (ordinaryUri != null) {
                val ordinary = app.openResource(
                    ordinaryUri,
                    resourceMimeType(canonical),
                    if (canonical.isDirectory) "directory" else "file",
                    packageName,
                )
                if (ordinary !is AndroidProviderResult.Failure ||
                    ordinary.code !in ELEVATED_FALLBACK_CODES
                ) {
                    return appResult(job, ordinary)
                }
            }
        }
        val elevated = selectedElevatedProvider()
        if (elevated == null) {
            return failure(
                job,
                APP_PROVIDER,
                CapabilityState.READY,
                if (canonical.exists()) "resource_permission_denied" else "resource_not_found",
                if (canonical.exists()) {
                    "Android could not grant this app access to the resource."
                } else {
                    "The requested resource does not exist."
                },
            )
        }
        val probe = elevated.awaitReady(context)
        if (probe.state != CapabilityState.READY) {
            return failure(
                job,
                elevated.providerId,
                probe.state,
                "capability_unavailable",
                "The selected authority is not ready.",
                retryable = probe.state != CapabilityState.UNSUPPORTED,
                requiredUserStep = probe.requiredUserStep,
            )
        }
        return openPathElevated(job, elevated, canonical, packageName)
    }

    private suspend fun openPathElevated(
        job: PhoneControlToolJobContext,
        provider: PrivilegedCommandProvider,
        file: File,
        packageName: String?,
    ): PhoneControlToolExecution {
        val exists = provider.executeAuthorized(
            context = context,
            effectOwner = job.effectOwner,
            program = TEST_PROGRAM,
            args = listOf("-e", file.absolutePath),
            cwd = ROOT_DIRECTORY,
            timeoutMs = RESOURCE_TIMEOUT_MS,
            effectMayChangeUserState = false,
        )
        currentCoroutineContext().ensureActive()
        when (exists) {
            is PrivilegedCommandResult.Failure -> {
                return privilegedFailure(job, provider.providerId, exists, 0)
            }
            is PrivilegedCommandResult.Success -> {
                if (!exists.receipt.isSuccessfulProcess()) {
                    return failure(
                        job,
                        provider.providerId,
                        CapabilityState.READY,
                        "resource_not_found",
                        "The requested resource is unavailable to the selected authority.",
                        observationGeneration = 0,
                    )
                }
            }
        }
        val uri = externalDocumentUri(context, file) ?: Uri.fromFile(file)
        val launched = provider.executeAuthorized(
            context = context,
            effectOwner = job.effectOwner,
            program = ACTIVITY_MANAGER,
            args = buildList {
                addAll(listOf(
                "start",
                "-W",
                "-a",
                Intent.ACTION_VIEW,
                "-d",
                uri.toString(),
                "-t",
                resourceMimeType(file),
                "-f",
                RESOURCE_INTENT_FLAGS,
                ))
                packageName?.let {
                    add("-p")
                    add(it)
                }
            },
            cwd = ROOT_DIRECTORY,
            timeoutMs = RESOURCE_TIMEOUT_MS,
            effectMayChangeUserState = true,
        )
        currentCoroutineContext().ensureActive()
        return elevatedLaunchResult(job, provider.providerId, 0, launched)
    }

    private fun ordinaryResourceUri(file: File): Uri? = if (file.isDirectory) {
        externalDocumentUri(context, file)
    } else {
        runCatching {
            FileProvider.getUriForFile(
                context,
                "${context.packageName}.fileprovider",
                file,
            )
        }.getOrNull()
    }

    private fun selectedElevatedProvider(): PrivilegedCommandProvider? {
        val selected = PhoneControlPowerPreferences.current(context)?.elevatedProviderId
            ?: return null
        if (selected !in PhoneControlProviderRegistry.providersFor(context, CAPABILITY)) return null
        return PrivilegedCommandProviderRegistry.find(selected)
    }

    private fun appResult(
        job: PhoneControlToolJobContext,
        result: AndroidProviderResult,
    ): PhoneControlToolExecution = providerResult(
        job = job,
        requestedTool = TOOL,
        capability = CAPABILITY,
        provider = APP_PROVIDER,
        mutating = true,
        invalidatesSnapshot = true,
        result = result,
    )

private fun invalidFailure(
    job: PhoneControlToolJobContext,
    argumentField: String,
    contractReason: String,
    message: String,
    code: String = "invalid_arguments",
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = TOOL,
        capability = CAPABILITY,
        provider = APP_PROVIDER,
        providerState = CapabilityState.READY,
        code = code,
        observationGeneration = 0,
        effect = EffectCertainty.PROVEN_NO_EFFECT,
        snapshotInvalidated = false,
        data = buildJsonObject {
            put("message", message)
            put("argument_field", argumentField)
            put("contract_reason", contractReason)
        },
    ),
    mutating = false,
    refreshScreenFrame = false,
)
}

internal fun shouldPreserveForegroundLaunch(
    packageName: String,
    observation: AccessibilityObservation,
): Boolean {
    val foreground = observation.windows.filter { window ->
        window.type == APPLICATION_WINDOW && window.active && window.focused &&
            !window.controllerOwned && !window.packageName.isNullOrBlank()
    }
    return foreground.singleOrNull()?.packageName == packageName
}

internal sealed interface ResourceLaunchInput {
    data class App(val name: String) : ResourceLaunchInput
    data class Content(val uri: Uri) : ResourceLaunchInput
    data class Path(val file: File) : ResourceLaunchInput
    data class Invalid(val code: String, val message: String) : ResourceLaunchInput
}

internal fun classifyResourceLaunchInput(
    raw: String,
    sharedStorageRoot: File,
): ResourceLaunchInput {
    val value = raw.trim()
    if (value.isEmpty()) {
        return ResourceLaunchInput.Invalid(
            "invalid_arguments",
            "launch_app requires a non-empty name.",
        )
    }
    when (val resolved = resolveAndroidFileListPath(value, sharedStorageRoot)) {
        is AndroidFileListPathResolution.Resolved -> return ResourceLaunchInput.Path(resolved.file)
        AndroidFileListPathResolution.Invalid -> Unit
    }
    val parsed = runCatching { URI(value) }.getOrNull()
        ?: return ResourceLaunchInput.App(value)
    return when (parsed.scheme?.lowercase(Locale.ROOT)) {
        "content" -> ResourceLaunchInput.Content(Uri.parse(value))
        "file" -> parsed.path
            ?.takeIf(String::isNotBlank)
            ?.let(::File)
            ?.takeIf(File::isAbsolute)
            ?.let(ResourceLaunchInput::Path)
            ?: ResourceLaunchInput.Invalid("invalid_arguments", "The file URI is invalid.")
        null -> ResourceLaunchInput.App(value)
        else -> ResourceLaunchInput.Invalid(
            "unsupported_scheme",
            "Use open_url for http(s); launch_app accepts apps and local resources.",
        )
    }
}

private fun elevatedLaunchResult(
    job: PhoneControlToolJobContext,
    providerId: String,
    observationGeneration: Long,
    result: PrivilegedCommandResult,
): PhoneControlToolExecution = when (result) {
    is PrivilegedCommandResult.Failure ->
        privilegedFailure(job, providerId, result, observationGeneration)
    is PrivilegedCommandResult.Success -> {
        val receipt = result.receipt
        val output = receipt["output"]?.jsonPrimitive?.contentOrNull.orEmpty()
        val code = receipt["code"]?.jsonPrimitive?.contentOrNull
        val exitCode = receipt["exit_code"]?.jsonPrimitive?.intOrNull
        when {
            code != "process_exited" || exitCode == null -> failure(
                job,
                providerId,
                CapabilityState.DEGRADED,
                code ?: "provider_transport_failure",
                "The selected authority did not return a terminal launch receipt.",
                observationGeneration,
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
                retryable = true,
            )
            output.contains("unable to resolve Intent", ignoreCase = true) -> failure(
                job,
                providerId,
                CapabilityState.READY,
                "no_handler",
                "No installed app can open this resource type.",
                observationGeneration,
            )
            exitCode != 0 || output.lineSequence().any { it.startsWith("Error:", true) } -> failure(
                job,
                providerId,
                CapabilityState.READY,
                "launch_failed",
                "Android did not complete the resource launch.",
                observationGeneration,
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
            )
            else -> success(job, providerId, observationGeneration)
        }
    }
}

private fun privilegedFailure(
    job: PhoneControlToolJobContext,
    providerId: String,
    result: PrivilegedCommandResult.Failure,
    observationGeneration: Long,
): PhoneControlToolExecution = failure(
    job = job,
    providerId = providerId,
    state = result.state,
    code = result.code,
    message = result.message,
    observationGeneration = observationGeneration,
    effect = if (result.effectMayHaveOccurred) {
        EffectCertainty.MAY_HAVE_OCCURRED
    } else {
        EffectCertainty.PROVEN_NO_EFFECT
    },
    retryable = result.state in RETRYABLE_PROVIDER_STATES,
    requiredUserStep = result.requiredUserStep,
    freshObservationRequired = result.freshObservationRequired,
)

private fun success(
    job: PhoneControlToolJobContext,
    providerId: String,
    observationGeneration: Long,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = TOOL,
        capability = CAPABILITY,
        provider = providerId,
        providerState = CapabilityState.READY,
        code = "ok",
        observationGeneration = observationGeneration,
        effect = EffectCertainty.MAY_HAVE_OCCURRED,
        snapshotInvalidated = true,
        freshObservationRequired = true,
        data = buildJsonObject { put("resource_kind", "path") },
    ),
    mutating = true,
    refreshScreenFrame = true,
)

private fun failure(
    job: PhoneControlToolJobContext,
    providerId: String,
    state: CapabilityState,
    code: String,
    message: String,
    observationGeneration: Long = 0,
    effect: EffectCertainty = EffectCertainty.PROVEN_NO_EFFECT,
    retryable: Boolean = false,
    requiredUserStep: String? = null,
    freshObservationRequired: Boolean = effect.effectMayHaveOccurred == true,
): PhoneControlToolExecution = PhoneControlToolExecution(
    response = toolResponse(
        job = job,
        requestedTool = TOOL,
        capability = CAPABILITY,
        provider = providerId,
        providerState = state,
        code = code,
        observationGeneration = observationGeneration,
        effect = effect,
        snapshotInvalidated = effect.effectMayHaveOccurred == true,
        retryable = retryable,
        requiredUserStep = requiredUserStep,
        freshObservationRequired = freshObservationRequired.takeIf { it },
        data = buildJsonObject { put("message", message) },
    ),
    mutating = effect.effectMayHaveOccurred == true,
    refreshScreenFrame = effect.effectMayHaveOccurred == true,
)

private fun JsonObject.isSuccessfulProcess(): Boolean =
    this["code"]?.jsonPrimitive?.contentOrNull == "process_exited" &&
        this["exit_code"]?.jsonPrimitive?.intOrNull == 0

private fun resourceMimeType(file: File): String = when {
    file.isDirectory -> DocumentsContract.Document.MIME_TYPE_DIR
    file.extension.equals("apk", ignoreCase = true) -> APK_MIME
    else -> MimeTypeMap.getSingleton()
        .getMimeTypeFromExtension(file.extension.lowercase(Locale.ROOT))
        ?: BINARY_MIME
}

private fun externalDocumentUri(context: Context, file: File): Uri? {
    val candidate = runCatching { file.canonicalFile }.getOrNull() ?: return null
    return storageVolumeRoots(context).firstNotNullOfOrNull { (volumeId, root) ->
        val canonicalRoot = runCatching { root.canonicalFile }.getOrNull()
            ?: return@firstNotNullOfOrNull null
        val rootPath = canonicalRoot.absolutePath.trimEnd(File.separatorChar)
        val candidatePath = candidate.absolutePath
        if (candidatePath != rootPath &&
            !candidatePath.startsWith("$rootPath${File.separator}")
        ) {
            return@firstNotNullOfOrNull null
        }
        val relative = candidatePath.removePrefix(rootPath)
            .trimStart(File.separatorChar)
            .replace(File.separatorChar, '/')
        val documentId = if (relative.isEmpty()) "$volumeId:" else "$volumeId:$relative"
        DocumentsContract.buildDocumentUri(EXTERNAL_STORAGE_AUTHORITY, documentId)
    }
}

@Suppress("DEPRECATION")
private fun storageVolumeRoots(context: Context): List<Pair<String, File>> {
    val storage = context.getSystemService(StorageManager::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && storage != null) {
        return storage.storageVolumes.mapNotNull { volume ->
            val root = volume.directory ?: return@mapNotNull null
            val id = if (volume.isPrimary) "primary" else volume.uuid ?: return@mapNotNull null
            id to root
        }
    }
    return listOf("primary" to Environment.getExternalStorageDirectory())
}

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

private val ELEVATED_FALLBACK_CODES = setOf("resource_permission_denied", "launch_failed")
private val RETRYABLE_PROVIDER_STATES = setOf(
    CapabilityState.DEGRADED,
    CapabilityState.NEEDS_USER_STEP,
    CapabilityState.REVOKED,
    CapabilityState.UNAVAILABLE,
)
private const val TOOL = "launch_app"
private const val CAPABILITY = "app_and_task_control"
private const val APP_PROVIDER = "android_app_api"
private const val TEST_PROGRAM = "/system/bin/test"
private const val ACTIVITY_MANAGER = "/system/bin/am"
private const val ROOT_DIRECTORY = "/"
private const val RESOURCE_TIMEOUT_MS = 10_000L
private const val RESOURCE_INTENT_FLAGS = "0x10000001"
private const val EXTERNAL_STORAGE_AUTHORITY = "com.android.externalstorage.documents"
private const val APK_MIME = "application/vnd.android.package-archive"
private const val BINARY_MIME = "application/octet-stream"

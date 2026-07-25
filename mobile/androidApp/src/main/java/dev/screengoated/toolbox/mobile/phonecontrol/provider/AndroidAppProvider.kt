package dev.screengoated.toolbox.mobile.phonecontrol.provider

import android.app.ActivityManager
import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.media.AudioManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.StatFs
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlin.coroutines.resume

internal class AndroidAppProvider(
    private val context: Context,
) {
    fun openUrl(url: String): AndroidProviderResult {
        val parsed = runCatching { Uri.parse(url.trim()) }.getOrNull()
            ?: return AndroidProviderResult.Failure("invalid_url", "The URL is invalid.")
        if (parsed.scheme !in setOf("http", "https")) {
            return AndroidProviderResult.Failure(
                "unsupported_url_scheme",
                "Only http and https URLs are accepted by this tool.",
            )
        }
        val intent = Intent(Intent.ACTION_VIEW, parsed)
            .addCategory(Intent.CATEGORY_BROWSABLE)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        return runCatching {
            context.startActivity(intent)
            AndroidProviderResult.Success(
                buildJsonObject {
                    put("opened_url", parsed.toString())
                    put("credential_context", "preferred_browser")
                },
                effectMayHaveOccurred = true,
            )
        }.getOrElse { error ->
            AndroidProviderResult.Failure("open_failed", error.message ?: "No browser accepted the URL.")
        }
    }

    fun launchApp(name: String): AndroidProviderResult {
        return when (val resolved = resolveExactLaunchablePackage(name)) {
            is AndroidAppPackageResolution.Failure -> AndroidProviderResult.Failure(
                resolved.code,
                resolved.message,
            )
            is AndroidAppPackageResolution.Resolved -> {
                val intent = context.packageManager.getLaunchIntentForPackage(resolved.packageName)
                    ?: return AndroidProviderResult.Failure(
                        "app_not_launchable",
                        "The matched app has no launch intent.",
                    )
                launchIntent(intent, resolved.packageName)
            }
        }
    }

    fun openResource(
        uri: Uri,
        mimeType: String,
        resourceKind: String,
        packageName: String? = null,
    ): AndroidProviderResult {
        val intent = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, mimeType)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION)
        packageName?.let(intent::setPackage)
        intent.clipData = ClipData.newRawUri("SGT Phone Control resource", uri)
        return try {
            context.startActivity(intent)
            AndroidProviderResult.Success(
                buildJsonObject {
                    put("resource_kind", resourceKind)
                    put("mime_type", mimeType)
                },
                effectMayHaveOccurred = true,
            )
        } catch (_: ActivityNotFoundException) {
            AndroidProviderResult.Failure(
                "no_handler",
                "No installed app can open this resource type.",
            )
        } catch (_: SecurityException) {
            AndroidProviderResult.Failure(
                "resource_permission_denied",
                "Android did not grant this app access to the resource.",
            )
        } catch (_: Throwable) {
            AndroidProviderResult.Failure(
                "launch_failed",
                "Android could not open the resource.",
            )
        }
    }

    fun resolveExactLaunchablePackage(name: String): AndroidAppPackageResolution {
        val requested = name.trim()
        if (requested.isEmpty()) {
            return AndroidAppPackageResolution.Failure(
                "invalid_request",
                "App name must not be empty.",
            )
        }
        val packageManager = context.packageManager
        packageManager.getLaunchIntentForPackage(requested)?.let {
            return AndroidAppPackageResolution.Resolved(requested)
        }
        val launcherQuery = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        val matches = packageManager.queryIntentActivities(launcherQuery, 0).filter { info ->
            val label = info.loadLabel(packageManager).toString()
            label.equals(requested, ignoreCase = true) ||
                info.activityInfo.packageName.equals(requested, ignoreCase = true)
        }
        return when (matches.size) {
            0 -> AndroidAppPackageResolution.Failure(
                "app_not_found",
                "No launchable app exactly matches the request.",
            )
            1 -> AndroidAppPackageResolution.Resolved(matches.single().activityInfo.packageName)
            else -> AndroidAppPackageResolution.Failure(
                "ambiguous_app",
                "More than one launchable app exactly matches the request.",
            )
        }
    }

    suspend fun systemQuery(domain: String, query: String): AndroidProviderResult = when (domain) {
        "capabilities" -> capabilityQuery(query)
        "audio" -> audioQuery(query)
        "clipboard" -> clipboardQuery(query)
        "process" -> processQuery(query)
        "storage" -> storageQuery(query)
        "window" -> AndroidProviderResult.Failure(
            "unsupported_query",
            "Window queries require the Accessibility surface provider.",
        )
        else -> AndroidProviderResult.Failure("unsupported_query", "Unknown system-query domain.")
    }

    fun writeClipboard(text: String): AndroidProviderResult {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
            ?: return AndroidProviderResult.Failure("capability_unavailable", "Clipboard service is unavailable.")
        clipboard.setPrimaryClip(ClipData.newPlainText("SGT Phone Control", text))
        return AndroidProviderResult.Success(
            buildJsonObject { put("characters", text.length) },
            effectMayHaveOccurred = true,
            effectVerified = true,
        )
    }

    private fun launchIntent(intent: Intent, packageName: String): AndroidProviderResult = runCatching {
        context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        AndroidProviderResult.Success(
            buildJsonObject { put("package", packageName) },
            effectMayHaveOccurred = true,
        )
    }.getOrElse { error ->
        AndroidProviderResult.Failure("launch_failed", error.message ?: "The app could not be launched.")
    }

    private fun capabilityQuery(query: String): AndroidProviderResult {
        if (query != "list") {
            return AndroidProviderResult.Failure("unsupported_query", "Unsupported capabilities query.")
        }
        return AndroidProviderResult.Success(
            buildJsonObject {
                put("accessibility", PhoneControlAccessibilityProvider.isReady)
                put("screenshot", dev.screengoated.toolbox.mobile.service.SgtAccessibilityService.canCaptureScreenshot)
                put("overlay", android.provider.Settings.canDrawOverlays(context))
                put("all_files", Build.VERSION.SDK_INT < Build.VERSION_CODES.R || Environment.isExternalStorageManager())
            },
        )
    }

    private fun audioQuery(query: String): AndroidProviderResult {
        if (query != "active_sessions") {
            return AndroidProviderResult.Failure("unsupported_query", "Unsupported audio query.")
        }
        val manager = context.getSystemService(AudioManager::class.java)
            ?: return AndroidProviderResult.Failure("capability_unavailable", "AudioManager is unavailable.")
        val devices = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            manager.getDevices(AudioManager.GET_DEVICES_OUTPUTS).map { device ->
                buildJsonObject {
                    put("id", device.id)
                    put("type", device.type)
                    put("name", device.productName.toString())
                }
            }
        } else {
            emptyList()
        }
        return AndroidProviderResult.Success(
            buildJsonObject {
                put("mode", manager.mode)
                put("music_volume", manager.getStreamVolume(AudioManager.STREAM_MUSIC))
                put("music_volume_max", manager.getStreamMaxVolume(AudioManager.STREAM_MUSIC))
                put("outputs", JsonArray(devices))
            },
        )
    }

    private suspend fun clipboardQuery(query: String): AndroidProviderResult {
        if (query != "text") {
            return AndroidProviderResult.Failure("unsupported_query", "Unsupported clipboard query.")
        }
        val service = dev.screengoated.toolbox.mobile.service.SgtAccessibilityService.instance
            ?: return AndroidProviderResult.Failure(
                "capability_unavailable",
                "Accessibility is required to read the foreground clipboard on this Android version.",
            )
        return suspendCancellableCoroutine { continuation ->
            service.readClipboardAsync { text ->
                if (continuation.isActive) {
                    continuation.resume(
                        AndroidProviderResult.Success(
                            buildJsonObject {
                                put("text", text.orEmpty())
                                put("available", text != null)
                            },
                        ),
                    )
                }
            }
        }
    }

    private fun processQuery(query: String): AndroidProviderResult {
        if (query != "list_basic") {
            return AndroidProviderResult.Failure("unsupported_query", "Unsupported process query.")
        }
        val manager = context.getSystemService(ActivityManager::class.java)
            ?: return AndroidProviderResult.Failure("capability_unavailable", "ActivityManager is unavailable.")
        val processes = manager.runningAppProcesses.orEmpty().map { process ->
            buildJsonObject {
                put("pid", process.pid)
                put("uid", process.uid)
                put("name", process.processName)
                put("importance", process.importance)
            }
        }
        return AndroidProviderResult.Success(
            buildJsonObject {
                put("visibility_scope", "android_app_visible_processes")
                put("processes", JsonArray(processes))
            },
        )
    }

    private fun storageQuery(query: String): AndroidProviderResult {
        if (query != "volumes") {
            return AndroidProviderResult.Failure("unsupported_query", "Unsupported storage query.")
        }
        val roots = listOfNotNull(context.filesDir, context.cacheDir, context.getExternalFilesDir(null))
        return AndroidProviderResult.Success(
            buildJsonObject {
                put(
                    "volumes",
                    buildJsonArray {
                        roots.distinctBy { it.absolutePath }.forEach { root ->
                            val stats = StatFs(root.absolutePath)
                            add(
                                buildJsonObject {
                                    put("path", root.absolutePath)
                                    put("available_bytes", stats.availableBytes)
                                    put("total_bytes", stats.totalBytes)
                                    put("readable", root.canRead())
                                    put("writable", root.canWrite())
                                },
                            )
                        }
                    },
                )
            },
        )
    }

}

internal sealed interface AndroidAppPackageResolution {
    data class Resolved(val packageName: String) : AndroidAppPackageResolution
    data class Failure(val code: String, val message: String) : AndroidAppPackageResolution
}

internal sealed interface AndroidProviderResult {
    data class Success(
        val data: JsonObject,
        val effectMayHaveOccurred: Boolean = false,
        val effectVerified: Boolean = false,
    ) : AndroidProviderResult

    data class Failure(
        val code: String,
        val message: String,
        val retryable: Boolean = false,
        val requiredUserStep: String? = null,
        val data: JsonObject = JsonObject(emptyMap()),
    ) : AndroidProviderResult
}

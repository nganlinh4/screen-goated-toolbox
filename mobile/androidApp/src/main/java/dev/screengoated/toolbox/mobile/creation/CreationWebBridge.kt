package dev.screengoated.toolbox.mobile.creation

import android.webkit.JavascriptInterface
import android.webkit.WebView
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.json.JSONObject

internal class CreationWebBridge(
    private val host: CreationPickerHost,
    private val tool: CreationTool,
    private val webView: WebView,
    private val manager: CreationJobManager,
    private val contextJson: String,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }

    @JavascriptInterface
    fun context(): String = contextJson

    @JavascriptInterface
    fun invoke(requestId: String, command: String, argsJson: String) {
        scope.launch {
            val args = runCatching { json.parseToJsonElement(argsJson).jsonObject }
                .getOrElse { JsonObject(emptyMap()) }
            when (command) {
                "pick_image", "pick_images" -> host.pickImages(requestId)
                "pick_output_dir" -> host.pickOutputDirectory(requestId)
                else -> runCommand(requestId, command, args)
            }
        }
    }

    fun resolvePicker(requestId: String, value: Any?) {
        val element = when (value) {
            null -> JsonNull
            is String -> JsonPrimitive(value)
            is List<*> -> JsonArray(value.filterIsInstance<String>().map(::JsonPrimitive))
            else -> JsonPrimitive(value.toString())
        }
        resolve(requestId, element)
    }

    fun rejectPicker(requestId: String, message: String) = reject(requestId, message)

    fun destroy() = scope.cancel()

    private fun runCommand(requestId: String, command: String, args: JsonObject) {
        when (command) {
            "close_window" -> {
                resolve(requestId, JsonNull)
                host.closeMiniApp()
            }
            "minimize_window" -> {
                resolve(requestId, JsonNull)
                host.minimizeMiniApp()
            }
            "start_drag" -> resolve(requestId, JsonNull)
            else -> scope.launch {
                runCatching { withContext(Dispatchers.IO) { dispatch(command, args) } }
                    .onSuccess { resolve(requestId, it) }
                    .onFailure { reject(requestId, it.message ?: "Creation request failed") }
            }
        }
    }

    private fun dispatch(command: String, args: JsonObject): JsonElement = when (command) {
        "default_output_dir" -> JsonPrimitive(manager.files.defaultOutputDirectoryLabel())
        "prepare_runtime" -> JsonPrimitive(manager.startPreparation(tool))
        "runtime_preparation_status" -> JsonPrimitive(manager.preparationStatus(tool))
        "start_job" -> json.encodeToJsonElement(
            CreationJobStatus.serializer(),
            manager.startJob(tool, args),
        )
        "segment_model" -> json.encodeToJsonElement(
            CreationJobStatus.serializer(),
            manager.startSegmentation(args.requiredString("continuationId")),
        )
        "job_status" -> json.encodeToJsonElement(
            CreationJobStatus.serializer(),
            manager.status(tool, args.string("jobId")),
        )
        "job_statuses" -> json.encodeToJsonElement(
            ListSerializer(CreationJobStatus.serializer()),
            manager.statuses(tool),
        )
        "cancel_job" -> {
            val statuses = manager.cancel(tool, args.string("jobId"))
            if (tool == CreationTool.IMAGE_TO_3D) {
                json.encodeToJsonElement(
                    CreationJobStatus.serializer(),
                    args.string("jobId")?.let { id -> statuses.firstOrNull { it.jobId == id } }
                        ?: statuses.lastOrNull()
                        ?: manager.status(tool, null),
                )
            } else {
                json.encodeToJsonElement(ListSerializer(CreationJobStatus.serializer()), statuses)
            }
        }
        "history_results" -> json.encodeToJsonElement(
            ListSerializer(CreationHistoryEntry.serializer()),
            manager.history.list(tool),
        )
        "rename_history_result" -> json.encodeToJsonElement(
            CreationHistoryEntry.serializer(),
            manager.renameHistory(
                tool,
                args.requiredString("id"),
                args.requiredString("newName"),
            ),
        )
        "delete_history_result" -> {
            manager.deleteHistory(tool, args.requiredString("id"))
            JsonNull
        }
        "read_asset" -> readAsset(args.requiredString("path"))
        "save_svg_edits" -> saveSvgEdits(
            args.requiredString("path"),
            args.requiredString("svg"),
        )
        "open_output" -> {
            args.string("path")?.takeIf(manager.files::exists)?.let(manager.files::openExternally)
            JsonNull
        }
        else -> error("Unknown command: $command")
    }

    private fun readAsset(path: String): JsonElement {
        val size = manager.files.size(path)
        require(size < 0 || size <= MAXIMUM_ASSET_BYTES) { "Preview asset is too large" }
        val mightBeSvg = tool == CreationTool.IMAGE_TO_SVG ||
            path.substringBefore('?').endsWith(".svg", ignoreCase = true)
        val bytes = if (mightBeSvg && (size < 0 || size <= MAXIMUM_SVG_BYTES)) {
            manager.files.readBytes(path, MAXIMUM_SVG_BYTES.toLong())
        } else null
        val asText = bytes?.let { runCatching { it.decodeToString() }.getOrNull()?.trimStart() }
        return if (asText?.startsWith("<svg", ignoreCase = true) == true) {
            buildJsonObject {
                put("text", asText)
                put("sizeBytes", bytes.size)
            }
        } else {
            buildJsonObject {
                put("dataUrl", manager.assets.register(path))
                put("sizeBytes", size)
            }
        }
    }

    private fun saveSvgEdits(path: String, svg: String): JsonElement {
        require(svg.length <= MAXIMUM_SVG_BYTES) { "Edited SVG is too large" }
        val lower = svg.lowercase()
        require(
            lower.contains("<svg") && lower.contains("</svg>") &&
                listOf("<script", "<foreignobject", "javascript:", " onload=", " onerror=")
                    .none(lower::contains),
        ) { "Edited SVG contains unsupported active content" }
        manager.files.writeText(path, svg)
        return buildJsonObject { put("sizeBytes", svg.length) }
    }

    private fun resolve(requestId: String, value: JsonElement) {
        val payload = json.encodeToString(JsonElement.serializer(), value)
        val script = "window.__sgtBridgeResolve(${JSONObject.quote(requestId)}, true, " +
            "${JSONObject.quote(payload)})"
        webView.post { webView.evaluateJavascript(script, null) }
    }

    private fun reject(requestId: String, message: String) {
        val script = "window.__sgtBridgeResolve(${JSONObject.quote(requestId)}, false, " +
            "${JSONObject.quote(message)})"
        webView.post { webView.evaluateJavascript(script, null) }
    }

    private companion object {
        const val MAXIMUM_ASSET_BYTES = 120L * 1024 * 1024
        const val MAXIMUM_SVG_BYTES = 20 * 1024 * 1024
    }
}

private fun JsonObject.string(key: String): String? = this[key]?.jsonPrimitive?.contentOrNull
private fun JsonObject.requiredString(key: String): String = string(key)
    ?.takeIf(String::isNotBlank)
    ?: error("$key is required")

package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Context
import android.os.Process
import android.os.SystemClock
import android.util.Log
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong

/** Diagnostics are evidence only and can never alter Phone Control state. */
internal object PhoneControlLog {
    @Volatile
    private var diagnosticDirectory: File? = null

    @Volatile
    private var diagnosticSessionId: String = "uninitialized"

    @Volatile
    private var diagnosticProcessRole = PhoneControlDiagnosticProcessRole.PRIMARY

    private val sequence = AtomicLong()
    private val writer = ThreadPoolExecutor(
        1,
        1,
        0L,
        TimeUnit.MILLISECONDS,
        LinkedBlockingQueue(MAX_PENDING_RECORDS),
        { task ->
            Thread(task, "phone-control-diagnostics").apply {
                isDaemon = true
                priority = Thread.MIN_PRIORITY
            }
        },
        ThreadPoolExecutor.DiscardOldestPolicy(),
    )

    fun initialize(
        context: Context,
        processRole: PhoneControlDiagnosticProcessRole =
            PhoneControlDiagnosticProcessRole.PRIMARY,
    ) {
        if (diagnosticDirectory != null) return
        synchronized(this) {
            if (diagnosticDirectory == null) {
                val directory = (
                    context.getExternalFilesDir(DIRECTORY_NAME)
                        ?: File(context.filesDir, DIRECTORY_NAME)
                    ).also { directory -> runCatching { directory.mkdirs() } }
                diagnosticProcessRole = processRole
                diagnosticSessionId =
                    "${processRole.wireName}-${Process.myPid()}-${SystemClock.elapsedRealtime()}"
                sequence.set(0)
                diagnosticDirectory = directory
            }
        }
        record("I", INTERNAL_TAG, "diagnostics_initialized")
    }

    fun d(tag: String, message: String): Int = write("D", tag, message)

    fun i(tag: String, message: String): Int = write("I", tag, message)

    fun w(tag: String, message: String): Int = write("W", tag, message)

    fun e(tag: String, message: String): Int = write("E", tag, message)

    fun e(tag: String, message: String, error: Throwable?): Int = write(
        level = "E",
        tag = tag,
        message = message,
        throwableType = error?.javaClass?.name,
        error = error,
    )

    private fun write(
        level: String,
        tag: String,
        message: String,
        throwableType: String? = null,
        error: Throwable? = null,
    ): Int {
        record(level, tag, message, throwableType)
        val safeTag = normalizeDiagnosticField(tag, MAX_TAG_CHARS)
        val safeSummary = consoleSummary(message, error)
        return runCatching {
            when (level) {
                "D" -> Log.d(safeTag, safeSummary)
                "I" -> Log.i(safeTag, safeSummary)
                "W" -> Log.w(safeTag, safeSummary)
                else -> Log.e(safeTag, safeSummary)
            }
        }.getOrDefault(0)
    }

    private fun record(
        level: String,
        tag: String,
        message: String,
        throwableType: String? = null,
    ) {
        val directory = diagnosticDirectory ?: return
        val timestamp = System.currentTimeMillis()
        val elapsed = SystemClock.elapsedRealtime()
        val sourceThread = Thread.currentThread().name.take(MAX_THREAD_CHARS)
        val safeTag = normalizeDiagnosticField(tag, MAX_TAG_CHARS)
        val parsed = parseDiagnosticEvent(
            normalizeDiagnosticField(message, MAX_MESSAGE_CHARS),
        )
        val safeThrowableType = throwableType?.let {
            normalizeDiagnosticField(it, MAX_THROWABLE_CHARS)
        }
        val recordSequence = sequence.incrementAndGet()
        runCatching {
            writer.execute {
                runCatching {
                    val record = JSONObject()
                        .put("schema_version", RECORD_SCHEMA_VERSION)
                        .put("process_role", diagnosticProcessRole.wireName)
                        .put("session_id", diagnosticSessionId)
                        .put("sequence", recordSequence)
                        .put("timestamp_ms", timestamp)
                        .put("elapsed_ms", elapsed)
                        .put("pid", Process.myPid())
                        .put("thread", sourceThread)
                        .put("level", level)
                        .put("tag", safeTag)
                        .put("event", parsed.name)
                        .put("fields", JSONObject(parsed.fields))
                    if (safeThrowableType != null) {
                        record.put("throwable_type", safeThrowableType)
                    }
                    appendRecord(directory, record.toString())
                }
            }
        }
    }

    private fun appendRecord(directory: File, json: String) {
        if (!directory.exists() && !directory.mkdirs()) return
        val current = File(directory, diagnosticProcessRole.currentFileName)
        if (current.length() >= MAX_FILE_BYTES) {
            val previous = File(directory, diagnosticProcessRole.previousFileName)
            previous.delete()
            if (!current.renameTo(previous)) current.delete()
        }
        FileOutputStream(current, true).bufferedWriter(Charsets.UTF_8).use { output ->
            output.append(json)
            output.newLine()
        }
    }

    internal fun normalizeDiagnosticField(value: String, maxChars: Int): String = value
        .asSequence()
        .map { character -> if (character.isISOControl()) ' ' else character }
        .joinToString(separator = "")
        .replace(WHITESPACE_RUN, " ")
        .trim()
        .take(maxChars)

    internal fun parseDiagnosticEvent(message: String): ParsedDiagnosticEvent {
        val tokens = message.split(' ').filter(String::isNotBlank)
        val event = tokens.firstOrNull()
            ?.takeIf { token -> EVENT_NAME.matches(token) && token in EVENT_NAMES }
            ?: FALLBACK_EVENT
        if (event == FALLBACK_EVENT) {
            return ParsedDiagnosticEvent(event, emptyMap())
        }
        val fields = linkedMapOf<String, Any>()
        tokens.drop(1).forEach { token ->
            val match = FIELD_TOKEN.matchEntire(token) ?: return@forEach
            val key = match.groupValues[1]
            val rawValue = match.groupValues[2]
            parseDiagnosticField(key, rawValue)?.let { value -> fields[key] = value }
        }
        return ParsedDiagnosticEvent(event, fields)
    }

    private fun parseDiagnosticField(key: String, rawValue: String): Any? = when {
        key in BOOLEAN_FIELD_NAMES && rawValue == "true" -> true
        key in BOOLEAN_FIELD_NAMES && rawValue == "false" -> false
        key in INTEGER_FIELD_NAMES -> rawValue.toLongOrNull()
        key in SYMBOL_FIELD_NAMES && SAFE_SYMBOL_VALUE.matches(rawValue) ->
            rawValue.take(MAX_FIELD_VALUE_CHARS)
        else -> null
    }

    internal fun consoleSummary(message: String, error: Throwable?): String {
        val parsed = parseDiagnosticEvent(
            normalizeDiagnosticField(message, MAX_MESSAGE_CHARS),
        )
        return buildString {
            append(parsed.name)
            parsed.fields.forEach { (key, value) ->
                append(' ').append(key).append('=').append(value)
            }
            error?.let { throwable ->
                append(" throwable_type=").append(throwable.javaClass.name)
                throwable.stackTrace.take(MAX_CONSOLE_STACK_FRAMES)
                    .forEachIndexed { index, frame ->
                        append(" frame_").append(index + 1).append('=')
                        append(
                            normalizeDiagnosticField(
                                "${frame.className}.${frame.methodName}:${frame.lineNumber}",
                                MAX_STACK_FRAME_CHARS,
                            ).replace(UNSAFE_STACK_TOKEN, "_"),
                        )
                    }
            }
        }
    }

    internal data class ParsedDiagnosticEvent(
        val name: String,
        val fields: Map<String, Any>,
    )

    private const val INTERNAL_TAG = "SGTPhoneControlDiagnostics"
    private const val DIRECTORY_NAME = "phone-control-diagnostics"
    internal const val RECORD_SCHEMA_VERSION = 3
    private const val MAX_PENDING_RECORDS = 512
    private const val MAX_FILE_BYTES = 1_048_576L
    private const val MAX_TAG_CHARS = 96
    private const val MAX_MESSAGE_CHARS = 1_024
    private const val MAX_THROWABLE_CHARS = 192
    private const val MAX_THREAD_CHARS = 96
    private const val MAX_FIELD_VALUE_CHARS = 256
    private const val MAX_CONSOLE_STACK_FRAMES = 6
    private const val MAX_STACK_FRAME_CHARS = 192
    private const val FALLBACK_EVENT = "diagnostic_event"
    private val EVENT_NAME = Regex("[A-Za-z][A-Za-z0-9_.-]*")
    private val FIELD_TOKEN = Regex("([A-Za-z][A-Za-z0-9_.-]*)=(\\S+)")
    private val SAFE_SYMBOL_VALUE = Regex("[A-Za-z][A-Za-z0-9_.:$-]*")
    private val UNSAFE_STACK_TOKEN = Regex("[^A-Za-z0-9_.:$-]")
    private val WHITESPACE_RUN = Regex(" +")
    private val EVENT_NAMES = setOf(
        "absorbed_tool_receipt",
        "activation_accessibility_reconnect_wait",
        "activation_service_start",
        "activation_step_complete",
        "activation_step_selected",
        "activation_stopped",
        "activation_user_step_opened",
        "activation_user_step_returned",
        "audio_uplink_started",
        "browser_turn_cleanup",
        "browser_connect_result",
        "browser_tunnel_result",
        "authority_result",
        "authority_setup_clear",
        "authority_setup_deferred",
        "authority_setup_dispatch",
        "authority_setup_event",
        "authority_setup_guidance",
        "authority_setup_navigation_retry",
        "authority_setup_progress",
        "authority_setup_result",
        "authority_setup_resume",
        "authority_setup_step",
        "authority_setup_waiting",
        "capture_resume_launcher_dispatched",
        "capture_resume_result",
        "capture_resume_skipped",
        "capture_resume_user_step_opened",
        "connect_result",
        "coordinator_open",
        "coordinator_reentry",
        "coordinator_reentry_ack",
        "diagnostics_initialized",
        "duplicate_tool_terminal_absorbed",
        "emotion_classification_applied",
        "emotion_classification_failed",
        "emotion_classification_requested",
        "execution_context",
        "external_step_result_ignored",
        "forget_result",
        "ignored_audio_part",
        "ignored_invalid_pcm",
        "invalidation_hard",
        "memory_append",
        "memory_finalize",
        "memory_revise",
        "microphone_capture_started",
        "microphone_capture_starting",
        "microphone_capture_retry",
        "microphone_speech_detected",
        "microphone_speech_ended",
        "microphone_speech_started",
        "optional_setup_result",
        "orb_dismiss",
        "orb_drag",
        "overlay_attach_failed",
        "overlay_attached",
        "overlay_host_changed",
        "overlay_state_sink_failed",
        "pair_result",
        "power_choice",
        "power_choice_route",
        "power_prompt",
        "power_prompt_attach_failed",
        "projection_attach",
        "projection_frame_decode_failed",
        "projection_frame_ready",
        "projection_resized",
        "projection_session_started",
        "projection_session_stopped",
        "projection_start_failed",
        "projection_terminal",
        "protected_checkpoint_enter",
        "protected_checkpoint_exit",
        "protected_checkpoint_monitor",
        "protected_checkpoint_navigation_retry",
        "protected_checkpoint_detected",
        "protected_checkpoint_boundary",
        "protected_setup_continue",
        "protected_setup_projection_resume",
        "protected_setup_result",
        "protocol_overflow_abandon",
        "provider_failure",
        "provider_operation_failed",
        "reconciliation_cleared",
        "renderer_gone",
        "renderer_ready",
        "renderer_recreate",
        "runtime_failed",
        "runtime_observer_failed",
        "runtime_released",
        "runtime_state",
        "screen_capture_degraded",
        "screen_capture_route",
        "screen_capture_waiting",
        "screen_uplink_started",
        "setup_session_state",
        "setup_voice_result",
        "screenshot_route",
        "screenshot_window_stale",
        "server_activity_started",
        "server_frame_gap",
        "live_session_opened",
        "service_command",
        "service_created",
        "service_bound",
        "service_destroyed",
        "service_start_failed",
        "settings_navigation",
        "sgt_adb_forget",
        "target_generation_mismatch",
        "target_lease_missing",
        "target_path_read_failed",
        "target_path_recovered",
        "target_resolution_failed",
        "tool_completion_queue_closed",
        "tool_dispatched",
        "tool_dispatch_failed",
        "tool_frame_rejected",
        "tool_receipt",
        "tool_rejection_overflow",
        "transport_closed",
        "transport_ready",
        "transport_receive_failed",
        "transport_reconnect",
        "transport_send_rejected",
        "transport_terminal_failure",
        "turn_completed",
        "turn_interrupted",
        "turn_started",
        "ui_goal_queued",
        "ui_goal_sent",
        "ui_goal_finished",
        "unparsed_server_frame",
        "visual_evidence_resumed",
        "visual_evidence_suspended",
        "visual_revision_mismatch",
        "window_dropped",
    )
    private val BOOLEAN_FIELD_NAMES = setOf(
        "accepted",
        "active",
        "active_root",
        "automation_requested",
        "checkpoint_monitoring",
        "capture_resume_dispatched",
        "capture_resume_requested",
        "checked",
        "committed",
        "completed",
        "connected",
        "content_present",
        "crashed",
        "deleted",
        "effect_verified",
        "ended",
        "external_step_active",
        "focused",
        "fresh_frame_requested",
        "fresh_observation_attached",
        "fresh_observation_required",
        "input_admitted",
        "announcement_pending",
        "listed_root",
        "overlay_mutated",
        "pairing_established",
        "ready",
        "recovered",
        "replaced",
        "requested",
        "retryable",
        "running",
        "runtime_alive",
        "runtime_reused",
        "snapshot_invalidated",
        "speech",
        "started",
        "state_reconciled",
        "tools",
        "truncated",
        "unavailable",
        "verified",
        "visible",
        "visual_evidence",
    )
    private val INTEGER_FIELD_NAMES = setOf(
        "acknowledgement_goal",
        "age_ms",
        "gap_ms",
        "open_ms",
        "frame",
        "argument_bytes",
        "argument_fields",
        "assistant_chars",
        "attempt",
        "attempted_observation_generation",
        "attempted_target_id",
        "attempted_visual_revision",
        "audio_frames",
        "automation_goal",
        "buffer_bytes",
        "bytes",
        "calls",
        "capture_generation",
        "consecutive_failures",
        "content_changes",
        "control_bytes",
        "control_count",
        "current_generation",
        "current_visual_revision",
        "density_dpi",
        "display",
        "display_id",
        "elapsed_ms",
        "element_count",
        "epoch",
        "event_type",
        "generation",
        "goal_id",
        "hard",
        "height",
        "index",
        "mapping_model_ms",
        "observation_generation",
        "pending",
        "periodic_frames",
        "pixel_revalidation_ms",
        "pixel_stride",
        "ready_providers",
        "propagation_ms",
        "rate",
        "reentry_sequence",
        "refresh_requests",
        "requested_count",
        "row_stride",
        "samples_per_frame",
        "screen_frames",
        "semantic_only",
        "semantic_since_hard",
        "server_frames",
        "start_id",
        "target_display_id",
        "target_generation",
        "target_id",
        "target_location_ms",
        "target_snapshot_generation",
        "target_verification_ms",
        "target_window_id",
        "tool_frames",
        "turn_id",
        "uid",
        "user_chars",
        "visited_nodes",
        "visual_revision",
        "window_changes",
        "width",
        "window",
        "window_count",
        "window_id",
        "unresolved_count",
        "verified_count",
    )
    private val SYMBOL_FIELD_NAMES = setOf(
        "action",
        "model",
        "rejected_token",
        "argument_field",
        "argument_keys",
        "authority",
        "automation_disposition",
        "capability",
        "capture_policy",
        "certainty",
        "choice",
        "code",
        "condition",
        "contract_reason",
        "decision",
        "grounding_stage",
        "effect_status",
        "exception",
        "feedback",
        "failure_class",
        "format",
        "host",
        "job_id",
        "kind",
        "mode",
        "name",
        "next",
        "origin",
        "outcome",
        "owner",
        "phase",
        "presentation",
        "privacy",
        "provider",
        "provider_role",
        "provider_route_error",
        "provider_state",
        "icon",
        "reason",
        "recovery",
        "required_user_step",
        "result",
        "route",
        "sgt_status",
        "source",
        "state",
        "step",
        "surface",
        "tool",
        "trigger",
        "type",
        "user_step",
    )
}

internal enum class PhoneControlDiagnosticProcessRole(
    val wireName: String,
    private val journalSuffix: String?,
) {
    PRIMARY("primary", null),
    AUTHORITY_BRIDGE("authority_bridge", "authority-bridge"),
    ;

    val currentFileName: String
        get() = journalSuffix?.let { "events.$it.jsonl" } ?: "events.jsonl"

    val previousFileName: String
        get() = journalSuffix?.let { "events.$it.previous.jsonl" }
            ?: "events.previous.jsonl"
}

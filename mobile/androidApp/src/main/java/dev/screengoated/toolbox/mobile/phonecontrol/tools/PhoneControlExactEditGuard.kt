package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlTurnRecorder
import java.io.File
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal data class ExactResourceRejection(val code: String, val message: String)

/** Canonical rejected-transaction state; platform snapshots own native leases. */
internal class PhoneControlExactEditGuard(
    private val identities: ExactResourceIdentitySource = AndroidExactResourceIdentitySource,
) : AutoCloseable, PhoneControlTurnRecorder {
    private data class Resource(
        val path: String, var snapshot: ExactResourceIdentity?, var identityError: String?,
        var tool: String, var code: String?, var misses: Int = 0, var structural: Int = 0,
    )
    private data class Pending(
        val turn: Long, val tool: String, val path: String,
        val snapshot: ExactResourceIdentity?, val error: String?,
    )
    private val resources = mutableListOf<Resource>()
    private var pending: Pending? = null
    private var turn: Long? = null
    private var retired = false
    private var closed = false

    @Synchronized
    fun beforeAction(job: PhoneControlToolJobContext, name: String, args: JsonObject): ExactResourceRejection? {
        if (closed || turn?.let { job.turnId < it } == true) {
            return ExactResourceRejection("ERR_CANCELLED", "The owning turn is stale or closed.")
        }
        if (turn != job.turnId) reset(job.turnId)
        pending?.snapshot?.close()
        pending = null
        if (retired) return ExactResourceRejection("ERR_CANCELLED", "The owning turn has retired.")
        if (name !in DEDICATED && name != "read_text_file") return null
        val path = args.string("path")?.takeIf { File(it).isAbsolute && '\u0000' !in it && !File(it).isDirectory }
            ?: return null
        val capture = runCatching { identities.capture(path) }
        val snapshot = capture.getOrNull()
        pending = Pending(job.turnId, name, path, snapshot, capture.exceptionOrNull()?.message)
        if (name != "read_text_file" && snapshot == null && resources.isNotEmpty()) {
            return ExactResourceRejection("ERR_EXACT_EDIT_RESOURCE_IDENTITY_UNAVAILABLE", "The active resource identity is unavailable; earlier evidence remains active.")
        }
        if (name == "edit_text_file" && snapshot != null && counts(snapshot).first >= 2) {
            return ExactResourceRejection("ERR_TEXT_FILE_REREAD_REQUIRED", "Read this file again before another exact replacement attempt.")
        }
        return null
    }

    @Synchronized
    fun needsScopeCheck(name: String, mutating: Boolean): Boolean =
        resources.isNotEmpty() && name !in EXACT_EDITS && mutating

    @Synchronized
    fun isCurrent(job: PhoneControlToolJobContext): Boolean = !closed && !retired && turn == job.turnId

    @Synchronized
    fun protectedResources(): JsonArray = JsonArray(resources.map { resource ->
        val current = runCatching { identities.capture(resource.path) }.getOrNull()
        try {
            buildJsonObject {
                put("requested_path", resource.path)
                put("rejected_tool", resource.tool)
                resource.code?.let { put("last_rejection_code", it) }
                resource.snapshot?.let { put("recorded_identity", it.description) }
                current?.let { put("current_identity", it.description) }
                resource.identityError?.let { put("identity_error", it) }
            }
        } finally { current?.close() }
    })

    @Synchronized
    fun pendingIdentity(): JsonObject? = pending?.let {
        buildJsonObject {
            put("requested_path", it.path)
            it.snapshot?.let { snapshot -> put("identity", snapshot.description) }
            it.error?.let { error -> put("identity_error", error) }
        }
    }

    @Synchronized
    fun recordResult(job: PhoneControlToolJobContext, name: String, args: JsonObject, result: JsonObject): JsonObject {
        val attempt = pending ?: return result
        pending = null
        if (retired || attempt.turn != turn || job.turnId != turn || attempt.tool != name || attempt.path != args.string("path")) {
            attempt.snapshot?.close()
            return result
        }
        val before = attempt.snapshot
        var retained = false
        try {
            if (name == "read_text_file") {
                if (result.string("code") != "ok" || before == null || !before.hasFile || counts(before).first == 0) return result
                val reconciled = withCurrent(attempt.path) { after ->
                    before.sameEntry(after) && before.sameFile(after) && after.whileReceiptMatches(result.string("sha256").orEmpty()) {
                        resources.filter { it.snapshot?.related(before) == true }.forEach { it.misses = 0 }
                    }
                }
                return JsonObject(result + ("resource_boundary_reconciled" to JsonPrimitive(reconciled)))
            }
            if (result.string("code") == "ok" && result.boolean("effect_verified") == true) {
                if (before == null || resources.none { it.snapshot?.related(before) == true }) return result
                val reconciled = withCurrent(attempt.path) { after ->
                    before.sameEntry(after) && after.hasFile &&
                        (result.boolean("atomic") == true || !before.hasFile || before.sameFile(after)) &&
                        after.whileReceiptMatches(result.string("sha256").orEmpty()) {
                            val inPlace = before.sameFile(after)
                            val removed = resources.filter { resource ->
                                resource.snapshot?.let { old ->
                                    old.sameEntry(before) || (inPlace && old.sameFile(before) && withCurrent(resource.path) {
                                        old.sameEntry(it) && it.sameFile(after)
                                    })
                                } == true
                            }
                            resources.removeAll(removed.toSet())
                            removed.forEach { it.snapshot?.close() }
                        }
                }
                return JsonObject(result + ("resource_boundary_reconciled" to JsonPrimitive(reconciled)))
            }
            val previousStructural = before?.let { counts(it).second } ?: 0
            val existing = resources.firstOrNull {
                if (it.snapshot == null && before == null) it.path == attempt.path
                else before != null && it.snapshot?.sameEntry(before) == true
            }
            val resource = existing ?: Resource(attempt.path, null, null, name, null).also(resources::add)
            resource.snapshot?.close()
            resource.snapshot = before
            retained = true
            resource.identityError = attempt.error
            resource.tool = name
            resource.code = result.string("code")
            if (resource.code == "ERR_TEXT_FILE_MATCH_MISSING") resource.misses = (resource.misses + 1).coerceAtMost(255)
            if (resource.code == "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED" ||
                (resource.code in STRUCTURAL_CODES && previousStructural > 0)) {
                resource.structural = (resource.structural + 1).coerceAtMost(255)
            }
            if (before != null && counts(before).second >= 3) return JsonObject(result + buildJsonObject {
                put("code", "ERR_TEXT_FILE_STRUCTURE_REJECTION_LIMIT")
                resource.code?.let { put("cause_code", it) }
                put("terminal_blocker", true)
                put("retryable", false)
            })
            return result
        } finally { if (!retained) before?.close() }
    }

    private fun counts(current: ExactResourceIdentity): Pair<Int, Int> {
        val related = resources.filter { it.snapshot?.related(current) == true }
        return related.sumOf { it.misses } to related.sumOf { it.structural }
    }

    private fun withCurrent(path: String, action: (ExactResourceIdentity) -> Boolean): Boolean =
        runCatching { identities.capture(path).use(action) }.getOrDefault(false)

    @Synchronized
    private fun reset(nextTurn: Long?, isRetired: Boolean = false) {
        pending?.snapshot?.close()
        pending = null
        resources.forEach { it.snapshot?.close() }
        resources.clear()
        turn = nextTurn
        retired = isRetired
    }

    @Synchronized override fun turnStarted(turnId: Long, generation: Long) {
        if (!closed && (turn == null || turnId > turn!!)) reset(turnId)
    }
    override fun userTranscriptUpdated(turnId: Long, text: String) = Unit
    override fun assistantTranscriptUpdated(turnId: Long, text: String) = Unit
    override fun turnCompleted(turnId: Long, userText: String, assistantText: String) = retire(turnId)
    override fun turnInterrupted(turnId: Long) = retire(turnId)
    @Synchronized private fun retire(turnId: Long) { if (turn == turnId) reset(turnId, true) }
    @Synchronized override fun close() { closed = true; reset(null, true) }

    companion object {
        private val EXACT_EDITS = setOf("edit_text_file", "edit_text_file_structure")
        private val DEDICATED = EXACT_EDITS + "save_artifact"
        private val STRUCTURAL_CODES = setOf("ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL", "ERR_TEXT_FILE_STRUCTURE_CHANGE", "ERR_TEXT_FILE_FORMULA_MIXED_EDIT")
    }
}

private fun ExactResourceIdentity.related(other: ExactResourceIdentity): Boolean = sameEntry(other) || sameFile(other)

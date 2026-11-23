package dev.screengoated.toolbox.mobile.phonecontrol.tools

import java.io.File
import java.util.UUID
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

/** Injected inode/entry transitions; native descriptors need device acceptance. */
internal class ExactResourceTestWorld : ExactResourceIdentitySource {
    data class Entry(val parent: String = "parent", val tail: String, val file: String?, val hash: String = HASH)
    val entries = mutableMapOf<String, Entry>()
    val unavailable = mutableSetOf<String>()
    var opened = 0
    val path = File(System.getProperty("java.io.tmpdir"), "sgt-resource-${UUID.randomUUID()}.csv").absolutePath
    val alias = "$path.alias"
    init { entries[path] = Entry(tail = "resource.csv", file = "inode-one") }

    override fun capture(path: String): ExactResourceIdentity {
        check(path !in unavailable) { "Native identity unavailable" }
        val entry = entries.getValue(path).copy()
        opened += 1
        return object : ExactResourceIdentity {
            var closed = false
            override val hasFile = entry.file != null
            override val description = buildJsonObject {
                put("parent", entry.parent); put("tail", entry.tail); entry.file?.let { put("file", it) }
            }
            override fun sameEntry(other: ExactResourceIdentity) =
                other.description["parent"] == description["parent"] && other.description["tail"] == description["tail"]
            override fun sameFile(other: ExactResourceIdentity) = entry.file != null && other.description["file"] == description["file"]
            override fun whileReceiptMatches(sha256: String, credit: () -> Unit): Boolean {
                if (closed || sha256.length != 64 || entries[path] != entry || sha256 != entry.hash || !hasFile) return false
                credit()
                return true
            }
            override fun close() { check(!closed); closed = true; opened -= 1 }
        }
    }
    fun args(path: String = this.path) = buildJsonObject { put("path", path) }
    fun receipt(atomic: Boolean = true, hash: String? = HASH) = buildJsonObject {
        put("code", "ok"); put("effect_verified", true); put("atomic", atomic)
        hash?.let { put("sha256", it) }
    }
    fun reject(code: String = "ERR_TEXT_FILE_MATCH_MISSING") = buildJsonObject { put("code", code) }
    companion object {
        val JOB = PhoneControlToolJobContext(1, "resource-job", 1)
        val HASH = "a".repeat(64)
        fun fixture(name: String): JsonObject {
            val relative = "parity-fixtures/phone-control/$name"
            val root = generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }.first { File(it, relative).isFile }
            return Json.parseToJsonElement(File(root, relative).readText()).jsonObject
        }
    }
}

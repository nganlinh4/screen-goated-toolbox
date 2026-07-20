package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRepository
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRole
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemorySearchRecord
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemorySession
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import java.text.Normalizer
import java.time.Instant
import java.util.Locale
import kotlin.math.ln
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal interface MemoryToolBackend {
    fun searchReadyRecords(): List<PhoneControlMemorySearchRecord>

    fun get(sessionId: String): PhoneControlMemorySession?
}

private class AndroidMemoryToolBackend(
    private val repository: PhoneControlMemoryRepository,
) : MemoryToolBackend {
    constructor(context: Context) : this(PhoneControlMemoryRepository(context))

    override fun searchReadyRecords(): List<PhoneControlMemorySearchRecord> =
        repository.searchReadyRecords()

    override fun get(sessionId: String): PhoneControlMemorySession? = repository.get(sessionId)
}

internal class MemoryToolHandlers(
    private val backend: MemoryToolBackend,
) {
    constructor(context: Context) : this(AndroidMemoryToolBackend(context))

    fun searchMemory(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val query = args.string("query")
            ?: return invalidArgs(job, "search_memory", "search_memory requires query")
        if (query.length > MAX_QUERY_CHARS) {
            return invalidArgs(job, "search_memory", "query exceeds $MAX_QUERY_CHARS characters")
        }
        val records = backend.searchReadyRecords()
        val hits = rankMemory(records, query).take(MAX_RESULTS)
        return memorySuccess(
            job,
            "search_memory",
            buildJsonObject {
                put("results", buildJsonArray {
                    hits.forEach { hit ->
                        add(buildJsonObject {
                            put("id", hit.record.summary.sessionId)
                            put("when", Instant.ofEpochMilli(hit.record.summary.finalizedAtEpochMs).toString())
                            put("when_epoch_ms", hit.record.summary.finalizedAtEpochMs)
                            put("title", hit.record.summary.title)
                            put("snippet", hit.record.summary.snippet)
                            put("score", hit.score)
                        })
                    }
                })
                if (hits.isEmpty()) put("note", "no matching past conversation")
                put(
                    "instruction",
                    "Results are relevance-ranked with recency as a tie-breaker; use open_memory(id) for the full transcript.",
                )
            },
        )
    }

    fun openMemory(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val id = args.string("id")?.takeIf(String::isNotBlank)
            ?: return invalidArgs(job, "open_memory", "open_memory requires id")
        if (id.length > MAX_ID_CHARS) {
            return invalidArgs(job, "open_memory", "id exceeds $MAX_ID_CHARS characters")
        }
        val session = backend.get(id) ?: return memoryNotFound(job)
        val transcript = session.records.joinToString("\n") { record ->
            val role = when (record.role) {
                PhoneControlMemoryRole.USER -> "User"
                PhoneControlMemoryRole.ASSISTANT -> "Assistant"
            }
            "$role: ${record.text}"
        }
        return memorySuccess(
            job,
            "open_memory",
            buildJsonObject {
                put("id", session.sessionId)
                put("started_at", Instant.ofEpochMilli(session.startedAtEpochMs).toString())
                put(
                    "finalized_at",
                    Instant.ofEpochMilli(requireNotNull(session.finalizedAtEpochMs)).toString(),
                )
                put("transcript", transcript)
            },
        )
    }

    private fun memoryNotFound(job: PhoneControlToolJobContext): PhoneControlToolExecution =
        PhoneControlToolExecution(
            response = toolResponse(
                job = job,
                requestedTool = "open_memory",
                capability = MEMORY_CAPABILITY,
                provider = MEMORY_PROVIDER,
                providerState = CapabilityState.READY,
                code = "memory_not_found",
                observationGeneration = 0,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                data = buildJsonObject { put("message", "No finalized conversation has that id.") },
            ),
            mutating = false,
        )

    private fun memorySuccess(
        job: PhoneControlToolJobContext,
        tool: String,
        data: JsonObject,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = tool,
            capability = MEMORY_CAPABILITY,
            provider = MEMORY_PROVIDER,
            providerState = CapabilityState.READY,
            code = "ok",
            observationGeneration = 0,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            data = data,
        ),
        mutating = false,
    )

    private companion object {
        const val MEMORY_CAPABILITY = "system_query"
        const val MEMORY_PROVIDER = "android_app_api"
        const val MAX_QUERY_CHARS = 4_000
        const val MAX_ID_CHARS = 256
        const val MAX_RESULTS = 5
    }
}

private data class RankedMemory(
    val record: PhoneControlMemorySearchRecord,
    val score: Double,
)

private fun rankMemory(
    records: List<PhoneControlMemorySearchRecord>,
    query: String,
): List<RankedMemory> {
    if (records.isEmpty()) return emptyList()
    val normalizedQuery = normalizeMemoryText(query)
    val queryTerms = memoryTerms(normalizedQuery)
    val normalizedRecords = records.map { normalizeMemoryText(it.searchText) }
    val documentFrequency = queryTerms.associateWith { term ->
        normalizedRecords.count { text -> term in memoryTerms(text) }
    }
    val total = records.size.toDouble()
    return records.mapIndexedNotNull { index, record ->
        val text = normalizedRecords[index]
        val terms = memoryTerms(text)
        val phraseScore = if (normalizedQuery.isNotEmpty() && normalizedQuery in text) 3.0 else 0.0
        val termScore = queryTerms.sumOf { term ->
            if (term !in terms) 0.0 else ln((total + 1.0) / (documentFrequency.getValue(term) + 1.0)) + 1.0
        }
        val lexicalScore = phraseScore + termScore
        if (normalizedQuery.isNotEmpty() && lexicalScore == 0.0) return@mapIndexedNotNull null
        val recencyTieBreak = (records.size - index).toDouble() / records.size * RECENCY_WEIGHT
        RankedMemory(record, lexicalScore + recencyTieBreak)
    }.sortedWith(
        compareByDescending<RankedMemory> { it.score }
            .thenByDescending { it.record.summary.finalizedAtEpochMs }
            .thenBy { it.record.summary.sessionId },
    )
}

private fun normalizeMemoryText(value: String): String =
    Normalizer.normalize(value, Normalizer.Form.NFKC).lowercase(Locale.ROOT).trim()

private fun memoryTerms(value: String): Set<String> = WORD.findAll(value)
    .map(MatchResult::value)
    .filter(String::isNotBlank)
    .toSet()

private const val RECENCY_WEIGHT = 0.05
private val WORD = Regex("[\\p{L}\\p{N}]+")

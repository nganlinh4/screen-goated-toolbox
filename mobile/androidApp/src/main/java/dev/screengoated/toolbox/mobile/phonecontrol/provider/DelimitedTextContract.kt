package dev.screengoated.toolbox.mobile.phonecontrol.provider

import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.security.MessageDigest
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal data class DelimitedTextAudit(
    val data: JsonObject,
)

internal data class DelimitedTextFailure(
    val code: String,
    val message: String,
    val data: JsonObject,
)

internal sealed interface DelimitedTextValidation {
    data object NotDelimited : DelimitedTextValidation
    data class Accepted(val audit: DelimitedTextAudit) : DelimitedTextValidation
    data class Rejected(val failure: DelimitedTextFailure) : DelimitedTextValidation
}

internal object DelimitedTextContract {
    fun validateOrdinary(
        file: File,
        before: String,
        after: String,
    ): DelimitedTextValidation {
        val format = DelimitedFormat.forFile(file) ?: return DelimitedTextValidation.NotDelimited
        val beforeProfile = parse(before, format.delimiter)
            ?: return rejected(
                code = "ERR_TEXT_FILE_STRUCTURE_BASELINE_UNREADABLE",
                message = "The original delimited text is not parseable, so record shape and formulas cannot be proved unchanged.",
                format = format,
            )
        val afterProfile = parse(after, format.delimiter)
            ?: return rejected(
                code = "ERR_TEXT_FILE_STRUCTURE_UNREADABLE",
                message = "The proposed ordinary edit makes the delimited text unparsable.",
                format = format,
                before = beforeProfile,
            )
        val shapePreserved = beforeProfile.fieldCounts == afterProfile.fieldCounts
        val formulasPreserved = beforeProfile.formulas == afterProfile.formulas
        if (
            shapePreserved &&
            !formulasPreserved &&
            mixedFormulaAndDataChange(beforeProfile, afterProfile)
        ) {
            return rejected(
                code = "ERR_TEXT_FILE_FORMULA_MIXED_EDIT",
                message = "Formula cells and ordinary data cannot change in one edit.",
                format = format,
                before = beforeProfile,
                after = afterProfile,
            )
        }
        if (!shapePreserved || !formulasPreserved) {
            return rejected(
                code = "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL",
                message = "The proposed ordinary edit changes record shape or formula cells.",
                format = format,
                before = beforeProfile,
                after = afterProfile,
            )
        }
        return DelimitedTextValidation.Accepted(
            DelimitedTextAudit(
                buildJsonObject {
                    put("format", format.wireName)
                    put("checked", true)
                    put("structural_change_confirmed", false)
                    put("record_shape_preserved", true)
                    put("formulas_preserved", true)
                    put("before_record_count", beforeProfile.fieldCounts.size)
                    put("after_record_count", afterProfile.fieldCounts.size)
                    put("before_formula_count", beforeProfile.formulas.size)
                    put("after_formula_count", afterProfile.formulas.size)
                },
            ),
        )
    }

    fun validateStructural(
        file: File,
        before: String,
        after: String,
        suppliedToken: String?,
    ): DelimitedTextValidation {
        val format = DelimitedFormat.forFile(file) ?: return DelimitedTextValidation.Rejected(
            DelimitedTextFailure(
                code = "ERR_TEXT_FILE_STRUCTURE_UNSUPPORTED",
                message = "Explicit structural text edits support only CSV and TSV files.",
                data = buildJsonObject { put("path", file.absolutePath) },
            ),
        )
        val expectedToken = structuralChangeToken(format, before, after)
        val beforeProfile = parse(before, format.delimiter)
        if (beforeProfile == null) {
            return if (suppliedToken == expectedToken) {
                uncheckedStructuralAudit(format, "The original delimited text is not parseable.")
            } else {
                rejected(
                    code = "ERR_TEXT_FILE_STRUCTURE_BASELINE_UNREADABLE",
                    message = "The original delimited text is not parseable.",
                    format = format,
                    confirmationToken = expectedToken,
                    parseError = "unterminated quoted field",
                )
            }
        }
        val afterProfile = parse(after, format.delimiter)
        if (afterProfile == null) {
            return if (suppliedToken == expectedToken) {
                uncheckedStructuralAudit(format, "The proposed delimited text is not parseable.")
            } else {
                rejected(
                    code = "ERR_TEXT_FILE_STRUCTURE_UNREADABLE",
                    message = "The proposed structural edit makes the delimited text unparsable.",
                    format = format,
                    before = beforeProfile,
                    confirmationToken = expectedToken,
                    parseError = "unterminated quoted field",
                )
            }
        }
        val shapePreserved = beforeProfile.fieldCounts == afterProfile.fieldCounts
        val formulasPreserved = beforeProfile.formulas == afterProfile.formulas
        if (
            shapePreserved &&
            !formulasPreserved &&
            mixedFormulaAndDataChange(beforeProfile, afterProfile)
        ) {
            return rejected(
                code = "ERR_TEXT_FILE_FORMULA_MIXED_EDIT",
                message = "Formula cells and ordinary data cannot change in one edit.",
                format = format,
                before = beforeProfile,
                after = afterProfile,
            )
        }
        if (shapePreserved && formulasPreserved) {
            return rejected(
                code = "ERR_TEXT_FILE_STRUCTURE_NOT_CHANGED",
                message = "The proposal changes only ordinary content; use edit_text_file.",
                format = format,
                before = beforeProfile,
                after = afterProfile,
            )
        }
        if (suppliedToken != expectedToken) {
            return rejected(
                code = "ERR_TEXT_FILE_STRUCTURE_CHANGE",
                message = "The proposal changes record shape or formula cells.",
                format = format,
                before = beforeProfile,
                after = afterProfile,
                confirmationToken = expectedToken,
            )
        }
        return DelimitedTextValidation.Accepted(
            DelimitedTextAudit(
                buildJsonObject {
                    put("format", format.wireName)
                    put("checked", true)
                    put("structural_change_confirmed", true)
                    put("record_shape_preserved", shapePreserved)
                    put("formulas_preserved", formulasPreserved)
                    put("before_record_count", beforeProfile.fieldCounts.size)
                    put("after_record_count", afterProfile.fieldCounts.size)
                    put("before_formula_count", beforeProfile.formulas.size)
                    put("after_formula_count", afterProfile.formulas.size)
                },
            ),
        )
    }

    private fun parse(text: String, delimiter: Char): DelimitedProfile? {
        val records = mutableListOf<List<String>>()
        var record = mutableListOf<String>()
        val field = StringBuilder()
        val formulas = mutableListOf<DelimitedFormulaCell>()
        var index = 0
        var fieldStart = 0
        var inQuotes = false
        var recordStarted = false
        fun finishField(end: Int) {
            val value = field.toString()
            if (value.trimStart().startsWith('=')) {
                formulas += DelimitedFormulaCell(records.size, record.size, text.substring(fieldStart, end))
            }
            record += value
            field.clear()
        }
        while (index < text.length) {
            val character = text[index++]
            if (inQuotes) {
                if (character == '"') {
                    if (text.getOrNull(index) == '"') {
                        field.append('"')
                        index += 1
                    } else {
                        inQuotes = false
                    }
                } else {
                    field.append(character)
                }
                recordStarted = true
                continue
            }
            when {
                character == '"' && field.isEmpty() -> {
                    inQuotes = true
                    recordStarted = true
                }
                character == delimiter -> {
                    finishField(index - 1)
                    fieldStart = index
                    recordStarted = true
                }
                character == '\r' || character == '\n' -> {
                    finishField(index - 1)
                    if (character == '\r' && text.getOrNull(index) == '\n') index += 1
                    fieldStart = index
                    records += record
                    record = mutableListOf()
                    recordStarted = false
                }
                else -> {
                    field.append(character)
                    recordStarted = true
                }
            }
        }
        if (inQuotes) return null
        if (recordStarted || field.isNotEmpty() || record.isNotEmpty()) {
            finishField(text.length)
            records += record
        }
        return DelimitedProfile(
            records = records,
            fieldCounts = records.map(List<String>::size),
            formulas = formulas,
        )
    }

    private fun mixedFormulaAndDataChange(
        before: DelimitedProfile,
        after: DelimitedProfile,
    ): Boolean {
        val formulaPositions = (before.formulas + after.formulas)
            .mapTo(mutableSetOf()) { it.record to it.field }
        return before.records.indices.any { record ->
            before.records[record].indices.any { field ->
                (record to field) !in formulaPositions &&
                    before.records[record][field] != after.records[record][field]
            }
        }
    }

    private fun rejected(
        code: String,
        message: String,
        format: DelimitedFormat,
        before: DelimitedProfile? = null,
        after: DelimitedProfile? = null,
        confirmationToken: String? = null,
        parseError: String? = null,
    ): DelimitedTextValidation.Rejected = DelimitedTextValidation.Rejected(
        DelimitedTextFailure(
            code,
            message,
            buildJsonObject {
                put("format", format.wireName)
                before?.let {
                    put("before_record_count", it.fieldCounts.size)
                    put("before_formula_count", it.formulas.size)
                    put("before_field_counts", JsonArray(it.fieldCounts.take(128).map(::JsonPrimitive)))
                }
                after?.let {
                    put("after_record_count", it.fieldCounts.size)
                    put("after_formula_count", it.formulas.size)
                    put("after_field_counts", JsonArray(it.fieldCounts.take(128).map(::JsonPrimitive)))
                }
                if (before != null && after != null) {
                    put("shape_mismatches", buildJsonArray {
                        val size = maxOf(before.fieldCounts.size, after.fieldCounts.size)
                        (0 until size).asSequence()
                            .filter { before.fieldCounts.getOrNull(it) != after.fieldCounts.getOrNull(it) }
                            .take(MAX_SHAPE_MISMATCHES)
                            .forEach { index ->
                                add(buildJsonObject {
                                    put("record_number", index + 1)
                                    before.fieldCounts.getOrNull(index)?.let { put("before_fields", it) }
                                    after.fieldCounts.getOrNull(index)?.let { put("after_fields", it) }
                                })
                            }
                    })
                }
                confirmationToken?.let { put("structural_change_token", it) }
                parseError?.let { put("parse_error", it) }
            },
        ),
    )

    private fun uncheckedStructuralAudit(
        format: DelimitedFormat,
        reason: String,
    ) = DelimitedTextValidation.Accepted(
        DelimitedTextAudit(
            buildJsonObject {
                put("format", format.wireName)
                put("checked", false)
                put("reason", reason)
                put("structural_change_confirmed", true)
            },
        ),
    )

    private fun structuralChangeToken(
        format: DelimitedFormat,
        before: String,
        after: String,
    ): String {
        val beforeBytes = before.toByteArray(Charsets.UTF_8)
        val afterBytes = after.toByteArray(Charsets.UTF_8)
        val digest = MessageDigest.getInstance("SHA-256")
        digest.update("sgt-edit-text-file-structure-v2\u0000".toByteArray(Charsets.UTF_8))
        digest.update(format.wireName.toByteArray(Charsets.UTF_8))
        digest.update(0)
        digest.update(littleEndianLength(beforeBytes.size))
        digest.update(beforeBytes)
        digest.update(littleEndianLength(afterBytes.size))
        digest.update(afterBytes)
        return digest.digest().joinToString("") { "%02x".format(it) }
    }

    private fun littleEndianLength(size: Int): ByteArray = ByteBuffer
        .allocate(Long.SIZE_BYTES)
        .order(ByteOrder.LITTLE_ENDIAN)
        .putLong(size.toLong())
        .array()

    private const val MAX_SHAPE_MISMATCHES = 16
}

internal enum class DelimitedFormat(
    val wireName: String,
    val delimiter: Char,
) {
    CSV("csv", ','),
    TSV("tsv", '\t');

    companion object {
        fun forFile(file: File): DelimitedFormat? = when (file.extension.lowercase()) {
            "csv" -> CSV
            "tsv" -> TSV
            else -> null
        }
    }
}

private data class DelimitedFormulaCell(
    val record: Int,
    val field: Int,
    val value: String,
)

private data class DelimitedProfile(
    val records: List<List<String>>,
    val fieldCounts: List<Int>,
    val formulas: List<DelimitedFormulaCell>,
)

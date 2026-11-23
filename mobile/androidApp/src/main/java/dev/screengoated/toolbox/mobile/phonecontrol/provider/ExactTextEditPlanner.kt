package dev.screengoated.toolbox.mobile.phonecontrol.provider

import java.io.File
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal data class PreparedTextEdit(
    val text: String,
    val replacementCount: Int,
    val replacementGroups: Int,
    val requestedReplacementGroups: Int,
    val formulaCellsAutoPreserved: Long,
    val formulaReplacementGroupsRewritten: Int,
    val formulaOnlyGroupsOmitted: Int,
    val trailingEmptyFieldsOmitted: Long,
    val trailingValueFieldsRepaired: Long,
    val structure: JsonObject?,
)

internal sealed interface TextEditPlan {
    data class Ready(val edit: PreparedTextEdit) : TextEditPlan
    data class Rejected(
        val code: String,
        val message: String,
        val data: JsonObject = JsonObject(emptyMap()),
    ) : TextEditPlan
}

internal object ExactTextEditPlanner {
    fun planOrdinary(
        file: File,
        original: String,
        requested: List<ExactReplacement>,
        bodyLimit: Long,
    ): TextEditPlan {
        validateRequest(requested)?.let { return it }
        val applied = when (
            val result = applyExact(original, requested, bodyLimit)
        ) {
            is ExactTextApplication.Ready -> result
            is ExactTextApplication.Rejected -> return result.failure
        }
        val structure = when (
            val validation = DelimitedTextContract.validateOrdinary(file, original, applied.text)
        ) {
            DelimitedTextValidation.NotDelimited -> null
            is DelimitedTextValidation.Accepted -> validation.audit.data
            is DelimitedTextValidation.Rejected -> {
                return TextEditPlan.Rejected(
                    validation.failure.code,
                    validation.failure.message,
                    validation.failure.data,
                )
            }
        }
        return TextEditPlan.Ready(
            PreparedTextEdit(
                text = applied.text,
                replacementCount = applied.replacementCount,
                replacementGroups = requested.size,
                requestedReplacementGroups = requested.size,
                formulaCellsAutoPreserved = 0,
                formulaReplacementGroupsRewritten = 0,
                formulaOnlyGroupsOmitted = 0,
                trailingEmptyFieldsOmitted = 0,
                trailingValueFieldsRepaired = 0,
                structure = structure,
            ),
        )
    }

    fun planStructural(
        file: File,
        original: String,
        requested: List<ExactReplacement>,
        suppliedToken: String?,
        bodyLimit: Long,
    ): TextEditPlan {
        validateRequest(requested)?.let { return it }
        val applied = when (val result = applyExact(original, requested, bodyLimit)) {
            is ExactTextApplication.Ready -> result
            is ExactTextApplication.Rejected -> return result.failure
        }
        val structure = when (
            val validation = DelimitedTextContract.validateStructural(
                file,
                original,
                applied.text,
                suppliedToken,
            )
        ) {
            DelimitedTextValidation.NotDelimited -> {
                return TextEditPlan.Rejected(
                    "ERR_TEXT_FILE_STRUCTURE_UNSUPPORTED",
                    "Explicit structural text edits support only CSV and TSV files.",
                )
            }
            is DelimitedTextValidation.Accepted -> validation.audit.data
            is DelimitedTextValidation.Rejected -> {
                return TextEditPlan.Rejected(
                    validation.failure.code,
                    validation.failure.message,
                    validation.failure.data,
                )
            }
        }
        return TextEditPlan.Ready(
            PreparedTextEdit(
                text = applied.text,
                replacementCount = applied.replacementCount,
                replacementGroups = requested.size,
                requestedReplacementGroups = requested.size,
                formulaCellsAutoPreserved = 0,
                formulaReplacementGroupsRewritten = 0,
                formulaOnlyGroupsOmitted = 0,
                trailingEmptyFieldsOmitted = 0,
                trailingValueFieldsRepaired = 0,
                structure = structure,
            ),
        )
    }

    private fun validateRequest(requested: List<ExactReplacement>): TextEditPlan.Rejected? {
        if (requested.size !in 1..MAX_EXACT_REPLACEMENT_GROUPS) {
            return TextEditPlan.Rejected(
                "ERR_TEXT_FILE_BAD_ARGUMENT",
                "The replacement list must contain 1 to $MAX_EXACT_REPLACEMENT_GROUPS items.",
            )
        }
        requested.forEachIndexed { index, replacement ->
            if (replacement.oldText.isEmpty() || replacement.expectedCount <= 0) {
                return TextEditPlan.Rejected(
                    "ERR_TEXT_FILE_BAD_ARGUMENT",
                    "Replacement ${index + 1} needs non-empty old text and a positive count.",
                )
            }
        }
        return null
    }

    private fun applyExact(
        original: String,
        replacements: List<ExactReplacement>,
        bodyLimit: Long,
    ): ExactTextApplication {
        val ranges = mutableListOf<PlannedReplacement>()
        replacements.forEachIndexed { replacementIndex, replacement ->
            val matches = exactMatchStarts(original, replacement.oldText, MAX_EXACT_MATCHES - ranges.size)
                ?: return ExactTextApplication.Rejected(
                    TextEditPlan.Rejected(
                        "ERR_TEXT_FILE_REPLACEMENT_LIMIT",
                        "The replacement request exceeds the total match limit.",
                    ),
                )
            if (matches.size != replacement.expectedCount) {
                return ExactTextApplication.Rejected(
                    TextEditPlan.Rejected(
                        if (matches.isEmpty()) {
                            "ERR_TEXT_FILE_MATCH_MISSING"
                        } else {
                            "ERR_TEXT_FILE_MATCH_AMBIGUOUS"
                        },
                        "An exact replacement count did not match the current file.",
                        buildJsonObject {
                            put("replacement_index", replacementIndex)
                            put("expected_count", replacement.expectedCount)
                            put("actual_count", matches.size)
                        },
                    ),
                )
            }
            matches.forEach { start ->
                ranges += PlannedReplacement(
                    start = start,
                    end = start + replacement.oldText.length,
                    replacement = replacement.newText,
                )
            }
        }
        ranges.sortWith(compareBy(PlannedReplacement::start, PlannedReplacement::end))
        if (ranges.zipWithNext().any { (left, right) -> right.start < left.end }) {
            return ExactTextApplication.Rejected(
                TextEditPlan.Rejected(
                    "ERR_TEXT_FILE_OVERLAPPING_REPLACEMENTS",
                    "Replacement match ranges overlap.",
                ),
            )
        }
        val size = BoundedUtf8Counter(bodyLimit)
        var cursor = 0
        ranges.forEach { planned ->
            if (!size.append(original, cursor, planned.start) ||
                !size.append(planned.replacement, 0, planned.replacement.length)
            ) {
                return ExactTextApplication.Rejected(size.failure())
            }
            cursor = planned.end
        }
        if (!size.append(original, cursor, original.length) || !size.complete()) {
            return ExactTextApplication.Rejected(size.failure())
        }
        val updated = buildString(original.length) {
            var cursor = 0
            ranges.forEach { planned ->
                append(original, cursor, planned.start)
                append(planned.replacement)
                cursor = planned.end
            }
            append(original, cursor, original.length)
        }
        if (updated == original) {
            return ExactTextApplication.Rejected(
                TextEditPlan.Rejected(
                    "ERR_TEXT_FILE_NO_CHANGE",
                    "The exact replacements would not change the file.",
                ),
            )
        }
        return ExactTextApplication.Ready(updated, ranges.size)
    }

    private fun exactMatchStarts(text: String, needle: String, limit: Int): List<Int>? {
        val matches = mutableListOf<Int>()
        var offset = 0
        while (offset <= text.length - needle.length) {
            val found = text.indexOf(needle, offset)
            if (found < 0) break
            if (matches.size >= limit) return null
            matches += found
            offset = found + needle.length
        }
        return matches
    }

    private const val MAX_EXACT_REPLACEMENT_GROUPS = 64
    private const val MAX_EXACT_MATCHES = 65_536

}

/** Counts the UTF-8 encoder's bytes without constructing the edited text. */
internal class BoundedUtf8Counter(private val limit: Long) {
    private var bytes = 0L
    private var highSurrogate = false
    private var invalid = false

    fun append(text: String, start: Int, end: Int): Boolean {
        for (index in start until end) {
            val character = text[index]
            when {
                character.isHighSurrogate() -> {
                    if (highSurrogate) {
                        invalid = true
                        return false
                    }
                    highSurrogate = true
                }
                character.isLowSurrogate() -> {
                    if (!highSurrogate) {
                        invalid = true
                        return false
                    }
                    bytes += 4
                    highSurrogate = false
                }
                else -> {
                    if (highSurrogate) {
                        invalid = true
                        return false
                    }
                    highSurrogate = false
                    bytes += when {
                        character.code < 0x80 -> 1
                        character.code < 0x800 -> 2
                        else -> 3
                    }
                }
            }
            if (bytes > limit) return false
        }
        return true
    }

    fun complete(): Boolean {
        if (highSurrogate) {
            invalid = true
            return false
        }
        return bytes <= limit
    }

    fun failure(): TextEditPlan.Rejected = if (invalid) {
        TextEditPlan.Rejected("ERR_TEXT_FILE_BAD_ARGUMENT", "The edited text is not valid Unicode.")
    } else {
        TextEditPlan.Rejected("ERR_TEXT_FILE_TOO_LARGE", "The edited file exceeds the bounded text limit.")
    }
}

private data class PlannedReplacement(
    val start: Int,
    val end: Int,
    val replacement: String,
)

private sealed interface ExactTextApplication {
    data class Ready(
        val text: String,
        val replacementCount: Int,
    ) : ExactTextApplication

    data class Rejected(val failure: TextEditPlan.Rejected) : ExactTextApplication
}

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
    ): TextEditPlan {
        validateRequest(requested)?.let { return it }
        val normalized = normalizeFormulaChanges(file, original, requested)
        if (normalized.replacements.isEmpty()) {
            return TextEditPlan.Rejected(
                "ERR_TEXT_FILE_NO_CHANGE",
                "The exact replacements would not change ordinary content.",
            )
        }
        val applied = when (
            val result = applyExact(original, normalized.replacements)
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
                replacementGroups = normalized.replacements.size,
                requestedReplacementGroups = requested.size,
                formulaCellsAutoPreserved = normalized.preservedCells,
                formulaReplacementGroupsRewritten = normalized.rewrittenGroups,
                formulaOnlyGroupsOmitted = normalized.omittedGroups,
                trailingEmptyFieldsOmitted = normalized.trailingEmptyFieldsOmitted,
                trailingValueFieldsRepaired = normalized.trailingValueFieldsRepaired,
                structure = structure,
            ),
        )
    }

    fun planStructural(
        file: File,
        original: String,
        requested: List<ExactReplacement>,
        suppliedToken: String?,
    ): TextEditPlan {
        validateRequest(requested)?.let { return it }
        val applied = when (val result = applyExact(original, requested)) {
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
    ): ExactTextApplication {
        val ranges = mutableListOf<PlannedReplacement>()
        replacements.forEachIndexed { replacementIndex, replacement ->
            val matches = exactMatchStarts(original, replacement.oldText)
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

    private fun normalizeFormulaChanges(
        file: File,
        original: String,
        requested: List<ExactReplacement>,
    ): NormalizedReplacements {
        val format = DelimitedTextContract.formatFor(file)
            ?: return NormalizedReplacements.unchanged(requested)
        var trailingEmptyFieldsOmitted = 0L
        var trailingValueFieldsRepaired = 0L
        val repaired = requested.map { replacement ->
            var normalized = replacement
            DelimitedTextRepair.trimRedundantTrailingEmptyFields(
                normalized.oldText,
                normalized.newText,
                format.delimiter,
            )?.let { repair ->
                normalized = normalized.copy(newText = repair.text)
                trailingEmptyFieldsOmitted +=
                    repair.count.toLong() * replacement.expectedCount.toLong()
            }
            DelimitedTextRepair.serializeSplitTrailingValues(
                normalized.oldText,
                normalized.newText,
                format.delimiter,
            )?.let { repair ->
                normalized = normalized.copy(newText = repair.text)
                trailingValueFieldsRepaired +=
                    repair.count.toLong() * replacement.expectedCount.toLong()
            }
            normalized
        }
        val originalCells = DelimitedTextContract.rawCells(original, format.delimiter)
        val analyses = repaired.map { replacement ->
            analyze(replacement, original, originalCells, format.delimiter)
        }
        val hasOrdinaryChange = analyses.any { it.kind == ChangeKind.DATA || it.kind == ChangeKind.MIXED }
        if (!hasOrdinaryChange) {
            return NormalizedReplacements.unchanged(
                repaired,
                trailingEmptyFieldsOmitted,
                trailingValueFieldsRepaired,
            )
        }

        val replacements = mutableListOf<ExactReplacement>()
        var preserved = 0L
        var rewritten = 0
        var omitted = 0
        repaired.zip(analyses).forEach { (replacement, analysis) ->
            when (analysis.kind) {
                ChangeKind.FORMULA_ONLY -> {
                    omitted += 1
                    preserved += analysis.formulaCells.toLong() * replacement.expectedCount.toLong()
                }
                ChangeKind.MIXED -> {
                    val patched = preserveFragmentFormulas(
                        replacement.oldText,
                        replacement.newText,
                        format.delimiter,
                    )
                    if (patched == null) {
                        replacements += replacement
                    } else {
                        replacements += replacement.copy(newText = patched.text)
                        preserved += patched.count.toLong() * replacement.expectedCount.toLong()
                        rewritten += 1
                    }
                }
                else -> replacements += replacement
            }
        }
        return NormalizedReplacements(
            replacements = replacements,
            preservedCells = preserved,
            rewrittenGroups = rewritten,
            omittedGroups = omitted,
            trailingEmptyFieldsOmitted = trailingEmptyFieldsOmitted,
            trailingValueFieldsRepaired = trailingValueFieldsRepaired,
        )
    }

    private fun analyze(
        replacement: ExactReplacement,
        original: String,
        originalCells: List<DelimitedRawCell>?,
        delimiter: Char,
    ): ReplacementAnalysis {
        if (replacement.oldText == replacement.newText) {
            return ReplacementAnalysis(ChangeKind.NONE, 0)
        }
        val rangeAnalysis = analyzeOriginalRanges(replacement, original, originalCells)
        if (rangeAnalysis.kind == ChangeKind.FORMULA_ONLY || rangeAnalysis.kind == ChangeKind.UNKNOWN) {
            return rangeAnalysis
        }
        return analyzeAligned(replacement, delimiter) ?: rangeAnalysis
    }

    private fun analyzeAligned(
        replacement: ExactReplacement,
        delimiter: Char,
    ): ReplacementAnalysis? {
        val oldCells = DelimitedTextContract.rawCells(replacement.oldText, delimiter)
            ?: return null
        val newCells = DelimitedTextContract.rawCells(replacement.newText, delimiter)
            ?: return null
        if (!align(oldCells, newCells)) return null
        var dataChanged = false
        var formulaChanged = false
        val formulaPositions = mutableSetOf<Pair<Int, Int>>()
        oldCells.zip(newCells).forEach { (oldCell, newCell) ->
            val oldRaw = replacement.oldText.substring(oldCell.start, oldCell.end)
            val newRaw = replacement.newText.substring(newCell.start, newCell.end)
            if (oldRaw == newRaw) return@forEach
            if (oldCell.formula || newCell.formula) {
                formulaChanged = true
                formulaPositions += oldCell.record to oldCell.field
            } else {
                dataChanged = true
            }
        }
        return ReplacementAnalysis(
            when {
                dataChanged && formulaChanged -> ChangeKind.MIXED
                dataChanged -> ChangeKind.DATA
                formulaChanged -> ChangeKind.FORMULA_ONLY
                else -> ChangeKind.NONE
            },
            formulaPositions.size,
        )
    }

    private fun analyzeOriginalRanges(
        replacement: ExactReplacement,
        original: String,
        originalCells: List<DelimitedRawCell>?,
    ): ReplacementAnalysis {
        if (originalCells == null) return ReplacementAnalysis(ChangeKind.UNKNOWN, 0)
        val matches = exactMatchStarts(original, replacement.oldText)
        if (matches.size != replacement.expectedCount) {
            return ReplacementAnalysis(ChangeKind.UNKNOWN, 0)
        }
        val formulaCells = originalCells.withIndex().filter { it.value.formula }
        val kinds = mutableSetOf<ChangeKind>()
        val touched = mutableSetOf<Int>()
        matches.forEach { start ->
            val end = start + replacement.oldText.length
            val overlapping = formulaCells.filter { (_, formula) ->
                start < formula.end && formula.start < end
            }
            overlapping.forEach { touched += it.index }
            kinds += when {
                overlapping.isEmpty() -> ChangeKind.DATA
                overlapping.any { (_, formula) -> formula.start <= start && end <= formula.end } ->
                    ChangeKind.FORMULA_ONLY
                else -> ChangeKind.MIXED
            }
        }
        return ReplacementAnalysis(
            kind = kinds.singleOrNull() ?: ChangeKind.UNKNOWN,
            formulaCells = touched.size,
        )
    }

    private fun preserveFragmentFormulas(
        old: String,
        new: String,
        delimiter: Char,
    ): DelimitedRepair? {
        val oldCells = DelimitedTextContract.rawCells(old, delimiter) ?: return null
        val newCells = DelimitedTextContract.rawCells(new, delimiter) ?: return null
        if (!align(oldCells, newCells)) {
            return DelimitedTextRepair.preserveSplitFormulaTails(
                old,
                new,
                oldCells,
                newCells,
            )
        }
        val patches = oldCells.zip(newCells).mapNotNull { (oldCell, newCell) ->
            if (!oldCell.formula) return@mapNotNull null
            val oldRaw = old.substring(oldCell.start, oldCell.end)
            val newRaw = new.substring(newCell.start, newCell.end)
            if (oldRaw == newRaw) null else RawTextPatch(newCell.start, newCell.end, oldRaw)
        }
        return DelimitedTextRepair.applyPatches(new, patches)
    }

    private fun align(
        old: List<DelimitedRawCell>,
        new: List<DelimitedRawCell>,
    ): Boolean = old.size == new.size && old.zip(new).all { (left, right) ->
        left.record == right.record && left.field == right.field
    }

    private fun exactMatchStarts(text: String, needle: String): List<Int> {
        val matches = mutableListOf<Int>()
        var offset = 0
        while (offset <= text.length - needle.length) {
            val found = text.indexOf(needle, offset)
            if (found < 0) break
            matches += found
            offset = found + needle.length
        }
        return matches
    }

    private const val MAX_EXACT_REPLACEMENT_GROUPS = 64
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

private data class NormalizedReplacements(
    val replacements: List<ExactReplacement>,
    val preservedCells: Long,
    val rewrittenGroups: Int,
    val omittedGroups: Int,
    val trailingEmptyFieldsOmitted: Long,
    val trailingValueFieldsRepaired: Long,
) {
    companion object {
        fun unchanged(
            replacements: List<ExactReplacement>,
            trailingEmptyFieldsOmitted: Long = 0,
            trailingValueFieldsRepaired: Long = 0,
        ) = NormalizedReplacements(
            replacements = replacements,
            preservedCells = 0,
            rewrittenGroups = 0,
            omittedGroups = 0,
            trailingEmptyFieldsOmitted = trailingEmptyFieldsOmitted,
            trailingValueFieldsRepaired = trailingValueFieldsRepaired,
        )
    }
}

private data class ReplacementAnalysis(
    val kind: ChangeKind,
    val formulaCells: Int,
)

private enum class ChangeKind {
    NONE,
    DATA,
    FORMULA_ONLY,
    MIXED,
    UNKNOWN,
}

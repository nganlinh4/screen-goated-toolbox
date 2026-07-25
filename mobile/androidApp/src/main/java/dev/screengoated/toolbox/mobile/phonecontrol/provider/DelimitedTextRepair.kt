package dev.screengoated.toolbox.mobile.phonecontrol.provider

internal data class DelimitedRepair(
    val text: String,
    val count: Int,
)

internal object DelimitedTextRepair {
    fun preserveSplitFormulaTails(
        old: String,
        new: String,
        oldCells: List<DelimitedRawCell>,
        newCells: List<DelimitedRawCell>,
    ): DelimitedRepair? {
        val oldRecords = recordSlices(oldCells)
        val newRecords = recordSlices(newCells)
        if (oldRecords.size != newRecords.size) return null
        val patches = mutableListOf<RawTextPatch>()
        oldRecords.zip(newRecords).forEach { (oldRecord, newRecord) ->
            if (oldRecord.size == newRecord.size) {
                oldRecord.zip(newRecord).forEach { (oldCell, newCell) ->
                    val oldRaw = old.substring(oldCell.start, oldCell.end)
                    val newRaw = new.substring(newCell.start, newCell.end)
                    if (oldCell.formula && oldRaw != newRaw) {
                        patches += RawTextPatch(newCell.start, newCell.end, oldRaw)
                    }
                }
                return@forEach
            }
            val oldFormula = oldRecord.lastOrNull() ?: return null
            if (
                !oldFormula.formula ||
                oldRecord.count(DelimitedRawCell::formula) != 1 ||
                newRecord.size <= oldFormula.field
            ) {
                return null
            }
            val prefixChanged = oldRecord
                .take(oldFormula.field)
                .zip(newRecord.take(oldFormula.field))
                .any { (oldCell, newCell) ->
                    oldCell.formula ||
                        newCell.formula ||
                        oldCell.field != newCell.field ||
                        oldCell.record != newCell.record
                }
            if (prefixChanged) return null
            val tailEnd = newRecord.lastOrNull()?.end ?: return null
            val tailStart = newRecord[oldFormula.field].start
            val originalFormula = old.substring(oldFormula.start, oldFormula.end)
            if (!sameFormulaTail(new.substring(tailStart, tailEnd), originalFormula)) return null
            patches += RawTextPatch(tailStart, tailEnd, originalFormula)
        }
        return applyPatches(new, patches)
    }

    fun trimRedundantTrailingEmptyFields(
        old: String,
        new: String,
        delimiter: Char,
    ): DelimitedRepair? {
        val oldCells = DelimitedTextContract.rawCells(old, delimiter) ?: return null
        val newCells = DelimitedTextContract.rawCells(new, delimiter) ?: return null
        val oldRecords = recordSlices(oldCells)
        val newRecords = recordSlices(newCells)
        if (oldRecords.size != newRecords.size) return null
        var dataChanged = false
        var omittedFields = 0
        val patches = mutableListOf<RawTextPatch>()
        oldRecords.zip(newRecords).forEach { (oldRecord, newRecord) ->
            if (oldRecord.isEmpty() || newRecord.size < oldRecord.size) return null
            dataChanged = dataChanged || oldRecord.zip(newRecord).any { (oldCell, newCell) ->
                !oldCell.formula &&
                    old.substring(oldCell.start, oldCell.end) !=
                    new.substring(newCell.start, newCell.end)
            }
            if (newRecord.size == oldRecord.size) return@forEach
            val extras = newRecord.drop(oldRecord.size)
            if (extras.any { cell ->
                    DelimitedTextContract.decodedField(new.substring(cell.start, cell.end))
                        .trim()
                        .isNotEmpty()
                }
            ) {
                return null
            }
            patches += RawTextPatch(
                start = newRecord[oldRecord.lastIndex].end,
                end = newRecord.lastOrNull()?.end ?: return null,
                value = "",
            )
            omittedFields += newRecord.size - oldRecord.size
        }
        if (!dataChanged || patches.isEmpty()) return null
        return applyPatches(new, patches)?.copy(count = omittedFields)
    }

    fun serializeSplitTrailingValues(
        old: String,
        new: String,
        delimiter: Char,
    ): DelimitedRepair? {
        val oldCells = DelimitedTextContract.rawCells(old, delimiter) ?: return null
        val newCells = DelimitedTextContract.rawCells(new, delimiter) ?: return null
        val oldRecords = recordSlices(oldCells)
        val newRecords = recordSlices(newCells)
        if (oldRecords.size != newRecords.size) return null
        var repairedFields = 0
        val patches = mutableListOf<RawTextPatch>()
        oldRecords.zip(newRecords).forEach { (oldRecord, newRecord) ->
            if (oldRecord.isEmpty() || newRecord.size < oldRecord.size) return null
            if (newRecord.size == oldRecord.size) return@forEach
            val trailingIndex = oldRecord.lastIndex
            val oldTrailing = oldRecord[trailingIndex]
            val prefixChanged = oldRecord
                .take(trailingIndex)
                .zip(newRecord.take(trailingIndex))
                .any { (oldCell, newCell) ->
                    DelimitedTextContract.decodedField(old.substring(oldCell.start, oldCell.end)) !=
                        DelimitedTextContract.decodedField(
                            new.substring(newCell.start, newCell.end),
                        )
                }
            if (oldTrailing.formula || prefixChanged) return null
            val tail = newRecord.drop(trailingIndex)
            val oldValue = DelimitedTextContract.decodedField(
                old.substring(oldTrailing.start, oldTrailing.end),
            )
            val firstValue = DelimitedTextContract.decodedField(
                new.substring(tail.first().start, tail.first().end),
            )
            if (
                firstValue == oldValue ||
                tail.drop(1).any { cell ->
                    DelimitedTextContract.decodedField(new.substring(cell.start, cell.end)).isEmpty()
                }
            ) {
                return null
            }
            val value = tail.joinToString(delimiter.toString()) { cell ->
                DelimitedTextContract.decodedField(new.substring(cell.start, cell.end))
            }
            patches += RawTextPatch(
                start = tail.first().start,
                end = tail.last().end,
                value = serializeField(value, delimiter),
            )
            repairedFields += newRecord.size - oldRecord.size
        }
        return applyPatches(new, patches)?.copy(count = repairedFields)
    }

    fun applyPatches(
        text: String,
        patches: List<RawTextPatch>,
    ): DelimitedRepair? {
        if (patches.isEmpty()) return null
        val ordered = patches.sortedBy(RawTextPatch::start)
        if (ordered.zipWithNext().any { (left, right) -> right.start < left.end }) return null
        val rewritten = StringBuilder(text)
        ordered.asReversed().forEach { patch ->
            rewritten.replace(patch.start, patch.end, patch.value)
        }
        return DelimitedRepair(rewritten.toString(), ordered.size)
    }

    private fun recordSlices(cells: List<DelimitedRawCell>): List<List<DelimitedRawCell>> =
        cells.groupBy(DelimitedRawCell::record).values.toList()

    private fun sameFormulaTail(candidate: String, original: String): Boolean {
        val decodedOriginal = DelimitedTextContract.decodedField(original)
        val candidateRaw = candidate.trim()
        val decodedCandidate = DelimitedTextContract.decodedField(candidateRaw)
        return decodedCandidate.trim() == decodedOriginal.trim() ||
            (
                !candidateRaw.startsWith('"') &&
                    decodedCandidate.replace("\"\"", "\"").trim() == decodedOriginal.trim()
            )
    }

    private fun serializeField(value: String, delimiter: Char): String =
        if (value.any { it == delimiter || it == '"' || it == '\r' || it == '\n' }) {
            "\"${value.replace("\"", "\"\"")}\""
        } else {
            value
        }
}

internal data class RawTextPatch(
    val start: Int,
    val end: Int,
    val value: String,
)

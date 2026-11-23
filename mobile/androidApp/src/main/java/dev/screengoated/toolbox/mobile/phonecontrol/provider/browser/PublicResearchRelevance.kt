package dev.screengoated.toolbox.mobile.phonecontrol.provider.browser

internal fun queryTerms(query: String): Set<String> =
    query.split(Regex("[^\\p{L}\\p{N}]+"))
        .map { it.lowercase() }
        .filter { it.codePointCount(0, it.length) >= 2 }
        .toSet()

internal fun researchMatchOffset(term: String, lowerText: String): Int? {
    var from = 0
    while (from <= lowerText.length) {
        val index = lowerText.indexOf(term, from)
        if (index < 0) return null
        val previous = if (index == 0) null else lowerText.codePointBefore(index)
        val category = previous?.let { Character.getType(it) }
        val previousAlphanumeric = previous != null && (Character.isLetter(previous) ||
            category == Character.DECIMAL_DIGIT_NUMBER.toInt() ||
            category == Character.LETTER_NUMBER.toInt() || category == Character.OTHER_NUMBER.toInt())
        if (term.codePointCount(0, term.length) > 2 || !previousAlphanumeric) return index
        from = index + 1
    }
    return null
}

internal fun matchingResearchTerms(terms: Set<String>, text: String): Int {
    val lower = text.lowercase()
    val exact = queryTerms(text)
    val exactShort = terms.count { it.codePointCount(0, it.length) == 2 && it in exact }
    return exactShort * (terms.size + 1) + terms.count { researchMatchOffset(it, lower) != null }
}

internal fun relevantExcerpt(text: String, query: String, maxChars: Int): String {
    if (maxChars <= 0) return ""
    if (text.codePointCount(0, text.length) <= maxChars) return text
    val terms = queryTerms(query)
    val blocks = text.split('\n').filter(String::isNotBlank)
    val best = blocks.maxByOrNull { block ->
        matchingResearchTerms(terms, block)
    }.orEmpty()
    val bestLength = best.codePointCount(0, best.length)
    if (bestLength >= maxChars) {
        val lower = best.lowercase()
        val lowerOffset = terms.mapNotNull { researchMatchOffset(it, lower) }.minOrNull()
        val focus = lowerOffset?.let { originalScalarIndex(best, it) } ?: 0
        val start = (focus - maxChars / 2).coerceIn(0, bestLength - maxChars)
        val first = best.offsetByCodePoints(0, start)
        return best.substring(first, best.offsetByCodePoints(first, maxChars))
    }
    val start = text.indexOf(best).coerceAtLeast(0)
    val availableBefore = text.codePointCount(0, start)
    val before = ((maxChars - bestLength) / 2).coerceAtMost(availableBefore)
    val first = text.offsetByCodePoints(start, -before)
    val available = text.codePointCount(first, text.length)
    val end = text.offsetByCodePoints(first, minOf(maxChars, available))
    return text.substring(first, end)
}

private fun originalScalarIndex(text: String, lowercaseOffset: Int): Int {
    var utf16Offset = 0
    var lowercaseLength = 0
    var scalarIndex = 0
    while (utf16Offset < text.length && lowercaseLength < lowercaseOffset) {
        val codePoint = text.codePointAt(utf16Offset)
        val width = Character.charCount(codePoint)
        lowercaseLength += text.substring(utf16Offset, utf16Offset + width).lowercase().length
        utf16Offset += width
        scalarIndex++
    }
    return scalarIndex
}

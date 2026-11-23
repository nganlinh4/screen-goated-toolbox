package dev.screengoated.toolbox.mobile.service.preset

/** Search only the given half-open range, including when the token is absent. */
internal fun indexOfHtmlTokenWithin(
    content: CharSequence,
    token: String,
    start: Int,
    end: Int,
): Int {
    require(start in 0..end && end <= content.length)
    if (token.isEmpty()) return start
    val last = end - token.length
    var index = start
    while (index <= last) {
        var offset = 0
        while (offset < token.length && content[index + offset] == token[offset]) offset++
        if (offset == token.length) return index
        index++
    }
    return -1
}

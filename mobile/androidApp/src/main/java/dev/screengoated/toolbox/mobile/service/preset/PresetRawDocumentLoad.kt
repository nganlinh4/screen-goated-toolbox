package dev.screengoated.toolbox.mobile.service.preset

internal enum class RawDocumentLoad { LOAD, KEEP, DEFER }

/**
 * A raw HTML result is a document lifecycle: it is loaded once and stays
 * authoritative. A document that is identical to the loaded one is kept, and a
 * document that completes while text is still streaming waits for the stream
 * to finish, so streamed or refined output never reloads the page per chunk.
 */
internal fun rawDocumentLoad(
    isStreaming: Boolean,
    loadedRawHtml: String?,
    html: String,
): RawDocumentLoad = when {
    loadedRawHtml == html -> RawDocumentLoad.KEEP
    isStreaming -> RawDocumentLoad.DEFER
    else -> RawDocumentLoad.LOAD
}

package dev.screengoated.toolbox.mobile.service.preset

internal data class TextInputEnvelope(val owner: String, val body: String)
internal data class TextInputSubmission(val owner: String, val revision: String, val text: String)

internal fun requireTextInputOwner(owner: String) {
    require(owner.isNotBlank() && ':' !in owner)
}

internal fun parseTextInputEnvelope(raw: String): TextInputEnvelope? {
    val boundary = raw.indexOf(':')
    if (boundary <= 0) return null
    return TextInputEnvelope(raw.substring(0,boundary),raw.substring(boundary+1))
}

internal fun parseTextInputSubmission(owner: String, body: String): TextInputSubmission? {
    val payload = body.removePrefix("submit:")
    if (payload == body) return null
    val boundary = payload.indexOf(':')
    if (boundary <= 0) return null
    val revision = payload.substring(0,boundary)
    if (revision.any { it !in '0'..'9' } || revision.toULongOrNull() == null) return null
    val text = payload.substring(boundary+1)
    if (text.isBlank()) return null
    return TextInputSubmission(owner,revision,text)
}

internal class TextInputDocumentScope {
    private var owner: String? = null
    private var closing = false
    val activeOwner: String? get() = owner.takeUnless { closing }

    fun open(owner: String) {
        requireTextInputOwner(owner)
        check(this.owner == null)
        this.owner = owner
        closing = false
    }

    fun accepts(source: String, envelope: TextInputEnvelope): Boolean =
        owner == source && envelope.owner == source && if (envelope.body == "input_exit_done") closing else !closing

    fun startClosing(): String? {
        val current = activeOwner ?: return null
        closing = true
        return current
    }

    fun retire(): String? {
        val active = activeOwner
        owner = null
        closing = false
        return active
    }
}

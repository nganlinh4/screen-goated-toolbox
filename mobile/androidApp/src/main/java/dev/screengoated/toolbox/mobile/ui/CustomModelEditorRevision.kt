package dev.screengoated.toolbox.mobile.ui

/** Tracks the newest snapshot already applied to the visible editor. */
internal class CustomModelEditorRevision(initial: Long) {
    private var applied = initial

    fun admit(revision: Long): Boolean {
        if (revision < applied) return false
        applied = revision
        return true
    }
}

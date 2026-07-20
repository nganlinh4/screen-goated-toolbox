package dev.screengoated.toolbox.mobile.phonecontrol.tools

import android.view.KeyEvent
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityKeyGroup

internal sealed interface AndroidKeySequenceParseResult {
    data class Success(val groups: List<AccessibilityKeyGroup>) : AndroidKeySequenceParseResult
    data class Unsupported(val token: String) : AndroidKeySequenceParseResult
    data class Invalid(val message: String) : AndroidKeySequenceParseResult
}

internal fun parseAndroidKeySequence(raw: String): AndroidKeySequenceParseResult {
    if (raw.isBlank()) return AndroidKeySequenceParseResult.Invalid("key combination is empty")
    if (raw.length > MAX_KEY_SEQUENCE_CHARS) {
        return AndroidKeySequenceParseResult.Invalid("key combination is too long")
    }
    val rawGroups = raw.split(',')
    if (rawGroups.size > MAX_KEY_GROUPS) {
        return AndroidKeySequenceParseResult.Invalid("too many sequential key groups")
    }
    val groups = mutableListOf<AccessibilityKeyGroup>()
    rawGroups.forEachIndexed { groupIndex, rawGroup ->
        if (rawGroup.isBlank()) {
            return AndroidKeySequenceParseResult.Invalid("empty key group at position ${groupIndex + 1}")
        }
        val tokens = rawGroup.split('+').map(String::trim)
        if (tokens.any(String::isEmpty)) {
            return AndroidKeySequenceParseResult.Invalid("empty key in group ${groupIndex + 1}")
        }
        if (tokens.size > MAX_KEYS_PER_GROUP) {
            return AndroidKeySequenceParseResult.Invalid("too many keys in group ${groupIndex + 1}")
        }
        val keyCodes = mutableListOf<Int>()
        for (token in tokens) {
            when (val parsed = androidKeyCode(token)) {
                is AndroidKeyCode.Exact -> keyCodes += parsed.keyCode
                is AndroidKeyCode.Unsupported -> return AndroidKeySequenceParseResult.Unsupported(token)
                AndroidKeyCode.Unknown -> {
                    return AndroidKeySequenceParseResult.Invalid("unknown Android key: $token")
                }
            }
        }
        if (keyCodes.toSet().size != keyCodes.size) {
            return AndroidKeySequenceParseResult.Invalid("duplicate key in group ${groupIndex + 1}")
        }
        groups += AccessibilityKeyGroup(keyCodes)
    }
    return AndroidKeySequenceParseResult.Success(groups)
}

private sealed interface AndroidKeyCode {
    data class Exact(val keyCode: Int) : AndroidKeyCode
    data object Unsupported : AndroidKeyCode
    data object Unknown : AndroidKeyCode
}

private fun androidKeyCode(token: String): AndroidKeyCode {
    val lower = token.lowercase()
    if (lower in DESKTOP_ONLY_KEYS) return AndroidKeyCode.Unsupported
    NAMED_KEYS[lower]?.let { return AndroidKeyCode.Exact(it) }
    if (lower.length == 1) {
        val character = lower.single()
        if (character in 'a'..'z') {
            return AndroidKeyCode.Exact(KeyEvent.KEYCODE_A + (character - 'a'))
        }
        if (character in '0'..'9') {
            return AndroidKeyCode.Exact(KeyEvent.KEYCODE_0 + (character - '0'))
        }
    }
    return AndroidKeyCode.Unknown
}

private val DESKTOP_ONLY_KEYS = setOf("win", "windows", "cmd", "command", "super")

private val NAMED_KEYS = mapOf(
    "ctrl" to KeyEvent.KEYCODE_CTRL_LEFT,
    "control" to KeyEvent.KEYCODE_CTRL_LEFT,
    "alt" to KeyEvent.KEYCODE_ALT_LEFT,
    "menu" to KeyEvent.KEYCODE_ALT_LEFT,
    "shift" to KeyEvent.KEYCODE_SHIFT_LEFT,
    "meta" to KeyEvent.KEYCODE_META_LEFT,
    "enter" to KeyEvent.KEYCODE_ENTER,
    "return" to KeyEvent.KEYCODE_ENTER,
    "tab" to KeyEvent.KEYCODE_TAB,
    "esc" to KeyEvent.KEYCODE_ESCAPE,
    "escape" to KeyEvent.KEYCODE_ESCAPE,
    "space" to KeyEvent.KEYCODE_SPACE,
    "spacebar" to KeyEvent.KEYCODE_SPACE,
    "backspace" to KeyEvent.KEYCODE_DEL,
    "delete" to KeyEvent.KEYCODE_FORWARD_DEL,
    "del" to KeyEvent.KEYCODE_FORWARD_DEL,
    "up" to KeyEvent.KEYCODE_DPAD_UP,
    "down" to KeyEvent.KEYCODE_DPAD_DOWN,
    "left" to KeyEvent.KEYCODE_DPAD_LEFT,
    "right" to KeyEvent.KEYCODE_DPAD_RIGHT,
    "move_home" to KeyEvent.KEYCODE_MOVE_HOME,
    "end" to KeyEvent.KEYCODE_MOVE_END,
    "pageup" to KeyEvent.KEYCODE_PAGE_UP,
    "pgup" to KeyEvent.KEYCODE_PAGE_UP,
    "pagedown" to KeyEvent.KEYCODE_PAGE_DOWN,
    "pgdn" to KeyEvent.KEYCODE_PAGE_DOWN,
    "f1" to KeyEvent.KEYCODE_F1,
    "f2" to KeyEvent.KEYCODE_F2,
    "f3" to KeyEvent.KEYCODE_F3,
    "f4" to KeyEvent.KEYCODE_F4,
    "f5" to KeyEvent.KEYCODE_F5,
    "f6" to KeyEvent.KEYCODE_F6,
    "f7" to KeyEvent.KEYCODE_F7,
    "f8" to KeyEvent.KEYCODE_F8,
    "f9" to KeyEvent.KEYCODE_F9,
    "f10" to KeyEvent.KEYCODE_F10,
    "f11" to KeyEvent.KEYCODE_F11,
    "f12" to KeyEvent.KEYCODE_F12,
)

private const val MAX_KEY_SEQUENCE_CHARS = 256
private const val MAX_KEY_GROUPS = 16
private const val MAX_KEYS_PER_GROUP = 8

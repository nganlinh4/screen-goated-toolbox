package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.Activity
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.text.InputType
import android.view.ViewGroup
import android.view.accessibility.AccessibilityEvent
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView

/** Shared debug-only native surface used to verify Phone Control device behavior. */
class PhoneControlAccessibilityFixtureActivity : Activity() {
    private lateinit var actionButton: Button
    private lateinit var status: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        status = TextView(this).apply {
            text = "Fixture ready"
        }
        actionButton = Button(this).apply {
            text = "Fixture action"
            contentDescription = INITIAL_ACTION_LABEL
        }
        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setBackgroundColor(Color.rgb(11, 23, 37))
                addView(
                    status,
                    ViewGroup.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.WRAP_CONTENT,
                    ),
                )
                addView(
                    actionButton,
                    ViewGroup.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.WRAP_CONTENT,
                    ),
                )
                addView(
                    EditText(this@PhoneControlAccessibilityFixtureActivity).apply {
                        inputType = InputType.TYPE_CLASS_TEXT or
                            InputType.TYPE_TEXT_VARIATION_PASSWORD
                        contentDescription = PROTECTED_FIELD_CANARY
                        hint = PROTECTED_FIELD_CANARY
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                            stateDescription = PROTECTED_FIELD_CANARY
                        }
                        setText(PROTECTED_FIELD_CANARY)
                    },
                    ViewGroup.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.WRAP_CONTENT,
                    ),
                )
            },
        )
    }

    fun mutateSurface() {
        status.text = "Fixture changed"
        actionButton.contentDescription = MUTATED_ACTION_LABEL
        actionButton.sendAccessibilityEvent(AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED)
    }

    companion object {
        const val INITIAL_ACTION_LABEL = "phone-control-fixture-action"
        const val MUTATED_ACTION_LABEL = "phone-control-fixture-action-updated"
        const val PROTECTED_FIELD_CANARY = "canary-device-password-48e2"
    }
}

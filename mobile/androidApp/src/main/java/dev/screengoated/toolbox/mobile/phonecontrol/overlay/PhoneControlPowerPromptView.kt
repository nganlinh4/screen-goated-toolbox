package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import android.content.Context
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.widget.LinearLayout
import android.widget.TextView
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerChoice

internal class PhoneControlPowerPromptView(
    context: Context,
    onChoice: (PhoneControlPowerChoice) -> Unit,
) : LinearLayout(context) {
    init {
        orientation = VERTICAL
        gravity = Gravity.START
        setPadding(dp(18), dp(16), dp(18), dp(16))
        background = GradientDrawable().apply {
            cornerRadius = dp(22).toFloat()
            setColor(Color.argb(242, 28, 30, 38))
            setStroke(dp(1), Color.argb(90, 180, 190, 220))
        }
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_YES
        contentDescription = context.getString(R.string.phone_control_power_prompt_title)

        addView(label(R.string.phone_control_power_prompt_title, 17f, Typeface.BOLD))
        addView(label(R.string.phone_control_power_prompt_message, 13f, Typeface.NORMAL).apply {
            setTextColor(Color.rgb(210, 214, 226))
            setPadding(0, dp(4), 0, dp(12))
        })
        addView(LinearLayout(context).apply {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER
            addChoice(
                R.string.phone_control_power_standard,
                PhoneControlPowerChoice.STANDARD,
                onChoice,
            )
            addChoice(
                R.string.phone_control_power_shizuku,
                PhoneControlPowerChoice.SHIZUKU,
                onChoice,
            )
            addChoice(
                R.string.phone_control_power_root,
                PhoneControlPowerChoice.ROOT,
                onChoice,
            )
        })
    }

    private fun label(resId: Int, size: Float, style: Int) = TextView(context).apply {
        setText(resId)
        textSize = size
        setTextColor(Color.WHITE)
        setTypeface(typeface, style)
    }

    private fun LinearLayout.addChoice(
        labelRes: Int,
        choice: PhoneControlPowerChoice,
        onChoice: (PhoneControlPowerChoice) -> Unit,
    ) {
        addView(TextView(context).apply {
            setText(labelRes)
            textSize = 13f
            gravity = Gravity.CENTER
            setTextColor(Color.WHITE)
            setTypeface(typeface, Typeface.BOLD)
            setPadding(dp(8), dp(10), dp(8), dp(10))
            background = GradientDrawable().apply {
                cornerRadius = dp(18).toFloat()
                setColor(Color.argb(160, 94, 82, 122))
            }
            isClickable = true
            isFocusable = true
            setOnClickListener { onChoice(choice) }
        }, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f).apply {
            marginStart = dp(3)
            marginEnd = dp(3)
        })
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()
}

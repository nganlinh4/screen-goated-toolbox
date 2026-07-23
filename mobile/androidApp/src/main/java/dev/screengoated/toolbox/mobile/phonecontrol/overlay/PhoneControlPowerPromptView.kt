package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.Drawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.view.Gravity
import android.widget.LinearLayout
import android.widget.TextView
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerChoice

internal class PhoneControlPowerPromptView(
    context: Context,
    onChoice: (PhoneControlPowerChoice) -> Unit,
    showForgetSgtAdb: Boolean,
    onForgetSgtAdb: () -> Unit,
) : LinearLayout(context) {
    init {
        orientation = VERTICAL
        gravity = Gravity.START
        setPadding(dp(12), dp(12), dp(12), dp(10))
        background = GradientDrawable().apply {
            cornerRadius = dp(20).toFloat()
            setColor(PANEL_COLOR)
            setStroke(dp(1), PANEL_STROKE_COLOR)
        }
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_YES
        contentDescription = context.getString(R.string.phone_control_power_prompt_title)

        addView(
            label(R.string.phone_control_power_prompt_title, 15.5f, Typeface.BOLD),
            LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT).apply {
                marginStart = dp(4)
                marginEnd = dp(4)
                bottomMargin = dp(9)
            },
        )
        addView(choiceRow(
            R.string.phone_control_power_standard to PhoneControlPowerChoice.STANDARD,
            R.string.phone_control_power_sgt_adb to PhoneControlPowerChoice.SGT_ADB,
            onChoice = onChoice,
        ), LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT).apply {
            bottomMargin = dp(6)
        })
        addView(choiceRow(
            R.string.phone_control_power_shizuku to PhoneControlPowerChoice.SHIZUKU,
            R.string.phone_control_power_root to PhoneControlPowerChoice.ROOT,
            onChoice = onChoice,
        ))
        if (showForgetSgtAdb) {
            addView(action(
                labelRes = R.string.phone_control_sgt_adb_forget,
                onClick = onForgetSgtAdb,
            ))
        }
    }

    private fun label(resId: Int, size: Float, style: Int) = TextView(context).apply {
        setText(resId)
        textSize = size
        setTextColor(Color.WHITE)
        setTypeface(typeface, style)
        includeFontPadding = false
    }

    private fun LinearLayout.addChoice(
        labelRes: Int,
        choice: PhoneControlPowerChoice,
        onChoice: (PhoneControlPowerChoice) -> Unit,
    ) {
        val recommended = choice == PhoneControlPowerChoice.SGT_ADB
        addView(TextView(context).apply {
            setText(labelRes)
            textSize = 13f
            gravity = Gravity.CENTER
            setTextColor(Color.WHITE)
            setTypeface(typeface, Typeface.BOLD)
            includeFontPadding = false
            minHeight = dp(46)
            setPadding(dp(8), 0, dp(8), 0)
            background = choiceBackground(recommended)
            if (recommended) {
                setCompoundDrawablesRelative(recommendedIcon(), null, null, null)
                compoundDrawablePadding = dp(5)
                contentDescription = context.getString(
                    R.string.phone_control_power_recommended,
                    context.getString(labelRes),
                )
            }
            isClickable = true
            isFocusable = true
            setOnClickListener { onChoice(choice) }
        }, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f).apply {
            marginStart = dp(3)
            marginEnd = dp(3)
        })
    }

    private fun choiceRow(
        vararg choices: Pair<Int, PhoneControlPowerChoice>,
        onChoice: (PhoneControlPowerChoice) -> Unit,
    ) = LinearLayout(context).apply {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER
        choices.forEach { (label, choice) -> addChoice(label, choice, onChoice) }
    }

    private fun action(labelRes: Int, onClick: () -> Unit) = TextView(context).apply {
        setText(labelRes)
        textSize = 11.5f
        gravity = Gravity.CENTER
        setTextColor(SECONDARY_TEXT_COLOR)
        includeFontPadding = false
        minHeight = dp(34)
        setPadding(dp(8), dp(5), dp(8), 0)
        background = subtleRipple()
        isClickable = true
        isFocusable = true
        setOnClickListener { onClick() }
    }

    private fun choiceBackground(recommended: Boolean): Drawable {
        val shape = GradientDrawable().apply {
            cornerRadius = dp(15).toFloat()
            if (recommended) {
                orientation = GradientDrawable.Orientation.LEFT_RIGHT
                colors = intArrayOf(RECOMMENDED_START_COLOR, RECOMMENDED_END_COLOR)
                setStroke(dp(1), RECOMMENDED_STROKE_COLOR)
            } else {
                setColor(CHOICE_COLOR)
                setStroke(dp(1), CHOICE_STROKE_COLOR)
            }
        }
        return RippleDrawable(
            ColorStateList.valueOf(Color.argb(52, 255, 255, 255)),
            shape,
            null,
        )
    }

    private fun subtleRipple(): Drawable {
        val content = GradientDrawable().apply {
            cornerRadius = dp(12).toFloat()
            setColor(Color.TRANSPARENT)
        }
        return RippleDrawable(
            ColorStateList.valueOf(Color.argb(34, 255, 255, 255)),
            content,
            null,
        )
    }

    private fun recommendedIcon(): Drawable? =
        context.getDrawable(R.drawable.ms_star)?.mutate()?.apply {
            setTint(RECOMMENDED_ICON_COLOR)
            setBounds(0, 0, dp(16), dp(16))
        }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    private companion object {
        val PANEL_COLOR: Int = Color.argb(248, 25, 25, 34)
        val PANEL_STROKE_COLOR: Int = Color.argb(105, 164, 170, 198)
        val CHOICE_COLOR: Int = Color.rgb(55, 50, 68)
        val CHOICE_STROKE_COLOR: Int = Color.argb(95, 160, 151, 185)
        val RECOMMENDED_START_COLOR: Int = Color.rgb(91, 77, 163)
        val RECOMMENDED_END_COLOR: Int = Color.rgb(127, 78, 178)
        val RECOMMENDED_STROKE_COLOR: Int = Color.rgb(226, 190, 105)
        val RECOMMENDED_ICON_COLOR: Int = Color.rgb(255, 216, 112)
        val SECONDARY_TEXT_COLOR: Int = Color.rgb(190, 194, 211)
    }
}

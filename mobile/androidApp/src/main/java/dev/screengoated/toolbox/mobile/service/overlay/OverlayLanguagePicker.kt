package dev.screengoated.toolbox.mobile.service.overlay

import android.content.Context
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.Rect
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.text.Editable
import android.text.TextUtils
import android.text.TextWatcher
import android.view.Gravity
import android.view.ViewGroup
import android.view.WindowManager
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import dev.screengoated.toolbox.mobile.service.OverlayBounds

internal class OverlayLanguagePicker(
    private val context: Context,
    private val windowManager: WindowManager,
    private val screenBoundsProvider: () -> Rect,
    private val onSelected: (String) -> Unit,
) {
    private var overlayView: FrameLayout? = null
    private val typeface: Typeface by lazy {
        runCatching {
            Typeface.createFromAsset(context.assets, "realtime_overlay/GoogleSansFlex.ttf")
        }.getOrDefault(Typeface.DEFAULT)
    }

    fun show(
        anchorBounds: OverlayBounds,
        selectedLanguage: String,
        languages: List<String>,
        isDark: Boolean,
        title: String,
    ) {
        hide()
        val screen = screenBoundsProvider()
        val margin = dp(16)
        val cardWidth = (screen.width() * 0.72f).toInt().coerceAtMost(dp(320))
        val cardHeight = (screen.height() * 0.58f).toInt().coerceAtMost(dp(440))
        val cardLeft = (anchorBounds.x + anchorBounds.width - cardWidth).coerceIn(
            margin,
            (screen.width() - cardWidth - margin).coerceAtLeast(margin),
        )
        val cardTop = (anchorBounds.y + dp(40)).coerceIn(
            margin,
            (screen.height() - cardHeight - margin).coerceAtLeast(margin),
        )

        val textColor = if (isDark) Color.parseColor("#F4F2F8") else Color.parseColor("#17151B")
        val subtextColor = if (isDark) Color.parseColor("#A19CA9") else Color.parseColor("#6E6874")

        val root = FrameLayout(context).apply {
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
            )
            setBackgroundColor(Color.argb(76, 0, 0, 0))
            isClickable = true
            setOnClickListener { hide() }
        }

        val card = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply {
                cornerRadius = dp(18).toFloat()
                setColor(if (isDark) Color.argb(250, 30, 30, 35) else Color.argb(250, 252, 248, 255))
                setStroke(dp(1), if (isDark) Color.parseColor("#40A8FF") else Color.parseColor("#66A8FF"))
            }
            elevation = dp(12).toFloat()
            setPadding(dp(14), dp(14), dp(14), dp(14))
            setOnClickListener { }
        }

        val header = TextView(context).apply {
            text = title
            setTextColor(textColor)
            textSize = 13f
            typeface = Typeface.create(this@OverlayLanguagePicker.typeface, Typeface.BOLD)
            setPadding(0, 0, 0, dp(8))
        }
        card.addView(
            header,
            LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT,
            ),
        )

        // Search box
        val searchBox = EditText(context).apply {
            hint = "Search..."
            setHintTextColor(subtextColor)
            setTextColor(textColor)
            textSize = 13f
            typeface = this@OverlayLanguagePicker.typeface
            isSingleLine = true
            imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN
            background = GradientDrawable().apply {
                cornerRadius = dp(12).toFloat()
                setColor(if (isDark) Color.argb(40, 255, 255, 255) else Color.argb(30, 0, 0, 0))
            }
            setPadding(dp(12), dp(8), dp(12), dp(8))
        }
        card.addView(
            searchBox,
            LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT,
            ).apply { bottomMargin = dp(8) },
        )

        val scrollView = ScrollView(context).apply {
            isFillViewport = true
            overScrollMode = ScrollView.OVER_SCROLL_IF_CONTENT_SCROLLS
        }
        val list = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
        }

        fun populateList(filter: String) {
            list.removeAllViews()
            val filtered = if (filter.isBlank()) {
                languages
            } else {
                languages.filter {
                    it.contains(filter, ignoreCase = true) ||
                        LanguageCatalog.codeForName(it).contains(filter, ignoreCase = true)
                }
            }
            for (language in filtered) {
                list.addView(languageRow(language, selectedLanguage == language, isDark))
            }
        }

        populateList("")

        searchBox.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) {
                populateList(s?.toString() ?: "")
            }
        })

        scrollView.addView(
            list,
            ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            ),
        )
        card.addView(
            scrollView,
            LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                0,
                1f,
            ),
        )

        root.addView(
            card,
            FrameLayout.LayoutParams(cardWidth, cardHeight).apply {
                gravity = Gravity.TOP or Gravity.START
                leftMargin = cardLeft
                topMargin = cardTop
            },
        )

        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.TOP or Gravity.START
            softInputMode = WindowManager.LayoutParams.SOFT_INPUT_ADJUST_PAN
        }

        overlayView = root
        windowManager.addView(root, params)
    }

    fun hide() {
        overlayView?.let { existing -> runCatching { windowManager.removeView(existing) } }
        overlayView = null
    }

    private fun languageRow(
        language: String,
        selected: Boolean,
        isDark: Boolean,
    ): LinearLayout {
        val accent = if (selected) Color.parseColor("#00C8FF") else Color.TRANSPARENT
        return LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = GradientDrawable().apply {
                cornerRadius = dp(14).toFloat()
                setColor(
                    when {
                        selected && isDark -> Color.argb(52, 0, 200, 255)
                        selected -> Color.argb(28, 0, 200, 255)
                        isDark -> Color.argb(26, 255, 255, 255)
                        else -> Color.argb(18, 0, 0, 0)
                    },
                )
                setStroke(dp(1), accent)
            }
            setPadding(dp(12), dp(11), dp(12), dp(11))
            setOnClickListener {
                onSelected(language)
                hide()
            }
            val params = LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT,
            )
            params.bottomMargin = dp(8)
            layoutParams = params

            addView(
                TextView(context).apply {
                    text = language
                    setTextColor(if (isDark) Color.parseColor("#F4F2F8") else Color.parseColor("#17151B"))
                    textSize = 13f
                    typeface = this@OverlayLanguagePicker.typeface
                    ellipsize = TextUtils.TruncateAt.END
                    maxLines = 1
                },
                LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f),
            )
            addView(
                TextView(context).apply {
                    text = LanguageCatalog.codeForName(language)
                    setTextColor(if (selected) Color.parseColor("#00C8FF") else if (isDark) Color.parseColor("#A19CA9") else Color.parseColor("#6E6874"))
                    textSize = 11f
                    typeface = Typeface.create(this@OverlayLanguagePicker.typeface, Typeface.BOLD)
                },
                LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                ),
            )
        }
    }

    private fun dp(value: Int): Int {
        return (value * context.resources.displayMetrics.density).toInt()
    }
}

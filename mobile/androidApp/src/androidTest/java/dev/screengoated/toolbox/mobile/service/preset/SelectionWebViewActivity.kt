package dev.screengoated.toolbox.mobile.service.preset

import android.app.Activity
import android.os.Bundle
import android.webkit.WebView
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

class SelectionWebViewActivity : Activity() {
    lateinit var web: WebView
        private set

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        web = WebView(this).apply {
            settings.javaScriptEnabled = true
            loadDataWithBaseURL("https://selection.test/", PAGE, "text/html", "utf-8", null)
        }
        val layout = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        val count = TextView(this).apply { text = "Native actions:0" }
        layout.addView(Button(this).apply {
            text = "Copy"
            setOnClickListener { count.text = "Native actions:1" }
        })
        layout.addView(count)
        layout.addView(web, LinearLayout.LayoutParams(-1, 0, 1f))
        setContentView(layout)
    }

    override fun onDestroy() {
        web.destroy()
        super.onDestroy()
    }

    companion object {
        const val PAGE = """<!doctype html><html><meta name="viewport" content="width=device-width">
            <body style="font-size:28px;padding:24px"><p id="selection">Lantern</p>
            <button onclick="document.getElementById('count').innerText='Page actions:1'">Copy</button>
            <p id="count">Page actions:0</p>
            </body></html>"""
    }
}

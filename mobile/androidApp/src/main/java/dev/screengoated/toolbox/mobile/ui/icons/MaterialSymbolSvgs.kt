package dev.screengoated.toolbox.mobile.ui.icons

import android.content.Context
import java.util.concurrent.ConcurrentHashMap

/**
 * Inline SVG markup for WebView surfaces, read from the downloaded Material Symbols in
 * `assets/material-symbols` (`bash scripts/download_material_symbols.sh <name>`).
 *
 * Every downloaded symbol is `width="1em" height="1em"` with `fill="currentColor"`, so it
 * follows the surrounding font size and text color unless a fixed pixel size is requested.
 */
internal class MaterialSymbolSvgs(
    private val readSymbolFile: (fileName: String) -> String,
) {
    private val sources = ConcurrentHashMap<String, String>()

    fun svg(name: String, sizePx: Int? = null): String {
        val source = sources.getOrPut(name) { readSymbolFile("$name.svg").trim() }
        return if (sizePx == null) {
            source
        } else {
            source.replaceFirst(CATALOG_SIZE, """width="$sizePx" height="$sizePx"""")
        }
    }

    /**
     * Fills the [slot] markers of HTML generated without a [Context]. A marker naming a
     * symbol that is not in the catalog resolves to nothing instead of failing the render.
     */
    fun resolveSlots(html: String): String = SLOT.replace(html) { match ->
        runCatching { svg(match.groupValues[1]) }.getOrDefault("")
    }

    companion object {
        const val ASSET_DIR = "material-symbols"
        private const val CATALOG_SIZE = """width="1em" height="1em""""
        private val NAME = Regex("[a-z0-9_]+")
        private val SLOT = Regex("<!--sgt-material-symbol:(${NAME.pattern})-->")

        fun fromAssets(context: Context): MaterialSymbolSvgs {
            val assets = context.applicationContext.assets
            return MaterialSymbolSvgs { fileName ->
                assets.open("$ASSET_DIR/$fileName").bufferedReader().use { it.readText() }
            }
        }

        /** An inert HTML comment that [resolveSlots] replaces with the named symbol. */
        fun slot(name: String): String {
            require(NAME.matches(name)) { "Invalid Material Symbol name: $name" }
            return "<!--sgt-material-symbol:$name-->"
        }
    }
}

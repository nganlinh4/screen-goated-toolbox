package dev.screengoated.toolbox.mobile.componentupdate

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class ComponentUpdatePaginationTest {
    @Test
    fun `catalog discovery ignores unrelated assets and traverses all pages`() {
        val requested = mutableListOf<Int>()
        val found = collectReleaseAssets { page ->
            requested.add(page)
            val count = if (page < 4) 100 else 2
            JSONArray().apply {
                repeat(count) { index ->
                    val name = if (page == 4 && index == 0) {
                        "sgt-component-catalog-v000001-0123456789abcdef.json"
                    } else "unrelated-$page-$index.zip"
                    put(JSONObject().put("name", name).put("size", 12).put("browser_download_url", "https://example.invalid/$name"))
                }
            }
        }
        assertEquals(listOf(1, 2, 3, 4), requested)
        assertEquals(listOf("sgt-component-catalog-v000001-0123456789abcdef.json"), found.map { it.name })
    }
}

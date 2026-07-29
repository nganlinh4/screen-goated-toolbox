package dev.screengoated.toolbox.mobile.creation

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationSvgSafetyTest {
    @Test
    fun `static renderer input preserves full safe SVG semantics`() {
        val svg = """
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
              <defs>
                <linearGradient id="g"><stop offset="0" stop-color="red"/></linearGradient>
                <pattern id="p" width="10" height="10" patternUnits="userSpaceOnUse">
                  <rect width="5" height="5" fill="blue"/>
                </pattern>
                <clipPath id="c"><circle cx="50" cy="50" r="40"/></clipPath>
                <mask id="m"><rect width="100" height="100" fill="white"/></mask>
                <path id="shape" d="M0 0L10 0L10 10Z"/>
              </defs>
              <use href="#shape" fill="url(#g)"/>
              <use xmlns:xlink="http://www.w3.org/1999/xlink" xlink:href="#shape"/>
              <rect width="100" height="100" fill="url(#p)" clip-path="url(#c)"
                    mask="url(#m)"/>
              <text x="10" y="90">Safe text</text>
            </svg>
        """.trimIndent()

        CreationArtifactValidator.validateSvgText(svg)
        listOf(
            "linearGradient",
            "pattern",
            "clipPath",
            "mask",
            "<use",
            "<text",
        ).forEach { token -> assertTrue(token in svg) }
    }

    @Test
    fun `validator rejects animation scripts and external CSS bypasses`() {
        val attacks = listOf(
            """<svg xmlns="http://www.w3.org/2000/svg"><animate attributeName="x"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><script>1</script></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><style>@import "https://x";</style></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><style>.x{fill:u/**/rl(https://x)}</style></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><style>.x{fill:u\72l(https://x)}</style></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><image href="file:///data/x"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><image src="#x"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg" xml:base="https://x/"/>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><feImage href="#x"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><filter id="x"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path filter="url(#x)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path style="filter:blur(2px)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path style="filter : blur(2px)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path style="f/**/ilter:blur(2px)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path style="f\69lter:blur(2px)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path style="f&#105;lter:blur(2px)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><style>.x{filter:blur(2px)}</style></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><canvas/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path style="fill:url(#ok);stroke:url(https://x)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path style="fill:image-set(url(https://x) 1x)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path style="fill:image-set('https://x/a.png' 1x)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path style="fill:-webkit-image-set('file:///x.png' 1x)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path fill="https://x"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path style="fill:u/*x*/rl(https://x)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><style>.x{fill:url(#paint)}</style>
                <linearGradient id="paint"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><style>
                @keyframes pulse{to{opacity:0}}.x{animation:pulse 1ms infinite}
               </style><path class="x"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><style>
                @-webkit-keyframes pulse{to{opacity:0}}.x{-webkit-animation-name:pulse}
               </style><path class="x"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg">
                <path style="transition:opacity 1ms"/>
               </svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg">
                <path style="-webkit-transition-property:opacity"/>
               </svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><g id="a"/><use href="#%61"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><g id="a"/><use href="#\61"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path fill="url(#%61)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><image href="data:image/webp;base64,AA=="/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><use href="data:image/png;base64,AA=="/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><a href="data:image/png;base64,AA=="/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path fill="url(data:image/png;base64,AA==)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path style="fill:url(data:image/png;base64,AA==)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2001/XInclude"><include href="#x"/></svg>""",
            """<svg xmlns="urn:not-svg"/>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path xmlns="urn:not-svg"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg" xmlns:other="urn:not-svg"/>""",
            """<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="urn:not-xlink"/>""",
            """<svg xmlns="HTTP://WWW.W3.ORG/2000/SVG"/>""",
            """<svg xmlns="http://www.w3.org/2000/svg" xmlns:other="urn:test">
                <path other:value="x"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"
                xmlns:XLINK="http://www.w3.org/1999/xlink">
                <use XLINK:href="#x"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><use xlink:href="#x"/></svg>""",
            """<!DOCTYPE svg [<!ENTITY x SYSTEM "file:///data/x">]>
                <svg xmlns="http://www.w3.org/2000/svg"><text>&x;</text></svg>""",
            """<?xml-stylesheet href="https://example.test/x.css"?>
                <svg xmlns="http://www.w3.org/2000/svg"/>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><?unsafe data?></svg>""",
        )

        attacks.forEachIndexed { index, svg ->
            assertThrows("Attack $index was accepted", Exception::class.java) {
                CreationArtifactValidator.validateSvgText(svg)
            }
        }
    }

    @Test
    fun `validator bounds geometry and rejects recursive local references`() {
        val pathBomb = "M".repeat(250_001)
        val identifierBomb = "x".repeat(513)
        val attacks = listOf(
            """<svg xmlns="http://www.w3.org/2000/svg"><path d="$pathBomb"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path d="M NaN Infinity"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><path transform="translate(1e99)"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg">
                <g id="a"><use href="#b"/></g><g id="b"><use href="#a"/></g>
               </svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg"><g id="same"/><g id="same"/></svg>""",
            """<svg xmlns="http://www.w3.org/2000/svg">
                <g id="$identifierBomb"/><use href="#$identifierBomb"/>
               </svg>""",
        )

        attacks.forEachIndexed { index, svg ->
            assertThrows("Geometry attack $index was accepted", Exception::class.java) {
                CreationArtifactValidator.validateSvgText(svg)
            }
        }
    }

    @Test
    fun `memoized local reference depth rejects shared suffix bypass`() {
        val suffix = (0..63).joinToString("") { index ->
            """<g id="n$index"><use href="#n${index + 1}"/></g>"""
        }
        val svg = """<svg xmlns="http://www.w3.org/2000/svg">
            <g id="short"><use href="#n63"/></g>
            <g id="long"><use href="#n0"/></g>
            $suffix
            <g id="n64"/>
        </svg>"""

        assertThrows(Exception::class.java) {
            CreationArtifactValidator.validateSvgText(svg)
        }
    }

    @Test
    fun `full fidelity surface stays inert and editor history stores deltas`() {
        val root = generateSequence(
            File(requireNotNull(System.getProperty("user.dir"))).absoluteFile,
        ) {
            it.parentFile
        }.first { File(it, ".claude/parity").isDirectory }
        val directory = File(
            root,
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation",
        )
        val surface = File(directory, "CreationSvgFullFidelitySurface.kt").readText()
        val editor = File(directory, "CreationSvgEditor.kt").readText()

        assertTrue("javaScriptEnabled = false" in surface)
        assertTrue("allowFileAccess = false" in surface)
        assertTrue("allowContentAccess = false" in surface)
        assertTrue("<body><img" in surface)
        assertTrue("shouldInterceptRequest" in surface)
        assertTrue("withContext(Dispatchers.Default)" in surface)
        assertTrue("data:image/svg+xml;base64" !in surface)
        assertTrue("Base64" !in surface)
        assertTrue("controller.zoom" in surface)
        assertTrue("controller.pan" in surface)
        assertTrue("SvgEditDelta" in editor)
        assertTrue("ArrayDeque<SvgSnapshot>" !in editor)
    }

    @Test
    fun `maximum presentation keeps html fixed size`() {
        val svg = "x".repeat(CreationContract.MAXIMUM_SVG_ARTIFACT_BYTES.toInt())
        val payload = createSvgPreviewPayload(svg)
        val html = svgPreviewHtml(payload.resourceUrl)

        assertTrue(payload.bytes.size == CreationContract.MAXIMUM_SVG_ARTIFACT_BYTES.toInt())
        assertTrue(html.length < 2_000)
        assertTrue(svg !in html)
    }

    @Test
    fun `total embedded raster budget charges every repeated occurrence`() {
        val pixels = CreationContract.MAXIMUM_SVG_EMBEDDED_RASTER_PIXELS
        val first = chargeCreationSvgEmbeddedRasterOccurrence(0, pixels)
        val second = chargeCreationSvgEmbeddedRasterOccurrence(first, pixels)

        assertEquals(CreationContract.MAXIMUM_SVG_TOTAL_EMBEDDED_RASTER_PIXELS, second)
        assertThrows(Exception::class.java) {
            chargeCreationSvgEmbeddedRasterOccurrence(second, 1)
        }

        val costs = mutableMapOf(
            CREATION_SVG_EXPANSION_ROOT to CreationSvgExpansionCost(
                elements = 4,
                rasterPixels = pixels,
                uses = mutableListOf("raster", "raster"),
            ),
            "raster" to CreationSvgExpansionCost(
                elements = 1,
                rasterPixels = pixels,
            ),
        )
        assertThrows(Exception::class.java) {
            validateCreationSvgExpansionCosts(costs)
        }
        costs.getValue(CREATION_SVG_EXPANSION_ROOT).uses.let { it.removeAt(it.lastIndex) }
        validateCreationSvgExpansionCosts(costs)

        val overReferenceLimit = mapOf(
            CREATION_SVG_EXPANSION_ROOT to CreationSvgExpansionCost(
                elements = 1,
                uses = MutableList(CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_EDGES + 1) {
                    "missing"
                },
            ),
        )
        assertThrows(Exception::class.java) {
            validateCreationSvgExpansionCosts(overReferenceLimit)
        }
    }

    @Test
    fun `multiplicity preserving local reference expansion rejects exponential DAG`() {
        val definitions = (0 until 16).joinToString("") { index ->
            """<pattern id="n$index">
                <rect fill="url(#n${index + 1})"/>
                <rect mask="url(#n${index + 1})"/>
               </pattern>"""
        }
        val svg = """<svg xmlns="http://www.w3.org/2000/svg">
            <defs>$definitions<pattern id="n16"><path d="M0 0h1"/></pattern></defs>
            <rect fill="url(#n0)"/>
        </svg>"""

        assertThrows(Exception::class.java) {
            CreationArtifactValidator.validateSvgText(svg)
        }
    }
}

package dev.screengoated.toolbox.mobile.creation

import java.io.ByteArrayInputStream
import java.util.Locale
import javax.xml.parsers.SAXParserFactory
import org.w3c.dom.Element
import org.w3c.dom.Node
import org.xml.sax.Attributes
import org.xml.sax.SAXException
import org.xml.sax.helpers.DefaultHandler

internal fun validateCreationSvgSecurityPreflight(bytes: ByteArray) {
    validateCreationSvgMarkupDeclarations(bytes)
    val factory = SAXParserFactory.newInstance().apply {
        isNamespaceAware = true
        runCatching {
            setFeature("http://apache.org/xml/features/disallow-doctype-decl", true)
        }
        runCatching {
            setFeature("http://xml.org/sax/features/external-general-entities", false)
        }
        runCatching {
            setFeature("http://xml.org/sax/features/external-parameter-entities", false)
        }
        runCatching {
            setFeature("http://apache.org/xml/features/nonvalidating/load-external-dtd", false)
        }
    }
    factory.newSAXParser().parse(ByteArrayInputStream(bytes), CreationSvgSecurityHandler())
}

internal fun validateCreationSvgMarkupDeclarations(bytes: ByteArray) {
    val text = bytes.decodeToString(throwOnInvalidSequence = true)
    require('\u0000' !in text) { "The vector result is invalid" }
    var cursor = if (text.startsWith('\uFEFF')) 1 else 0
    if (text.startsWith("<?xml", cursor)) {
        val end = text.indexOf("?>", cursor + 5)
        require(end in (cursor + 6)..(cursor + 256)) { "The vector result is invalid" }
        val declaration = text.substring(cursor, end + 2)
        val encoding = XML_DECLARATION_ENCODING.find(declaration)?.groupValues?.get(2)
        require(encoding == null || encoding.equals("utf-8", ignoreCase = true)) {
            "The vector result is invalid"
        }
        cursor = end + 2
    }
    while (true) {
        val start = text.indexOf('<', cursor)
        if (start < 0) return
        when {
            text.startsWith("<!--", start) -> {
                cursor = text.indexOf("-->", start + 4)
                require(cursor >= 0) { "The vector result is invalid" }
                cursor += 3
            }
            text.startsWith("<![CDATA[", start) -> {
                cursor = text.indexOf("]]>", start + 9)
                require(cursor >= 0) { "The vector result is invalid" }
                cursor += 3
            }
            text.startsWith("<!", start) || text.startsWith("<?", start) ->
                error("The vector result contains active XML declarations")
            else -> cursor = start + 1
        }
    }
}

private class CreationSvgSecurityHandler : DefaultHandler() {
    private var elements = 0
    private var attributes = 0
    private var depth = 0
    private var geometryTokens = 0
    private var pathCommands = 0

    override fun startElement(
        uri: String?,
        localName: String?,
        qName: String?,
        values: Attributes,
    ) {
        elements += 1
        attributes += values.length
        depth += 1
        rejectSvgComplexity(
            elements <= CreationContract.MAXIMUM_SVG_ELEMENTS &&
                attributes <= CreationContract.MAXIMUM_SVG_ATTRIBUTES &&
                depth <= CREATION_SVG_MAXIMUM_DOCUMENT_DEPTH,
        )
        val tag = (localName?.takeIf(String::isNotBlank) ?: qName.orEmpty())
            .substringAfter(':')
            .lowercase(Locale.ROOT)
        rejectSvgComplexity(tag != "filter" && !tag.startsWith("fe"))
        for (index in 0 until values.length) {
            val name = (values.getLocalName(index).takeIf(String::isNotBlank)
                ?: values.getQName(index)).substringAfter(':').lowercase(Locale.ROOT)
            val value = values.getValue(index).orEmpty()
            rejectSvgComplexity(value.length <= maximumSvgAttributeCharacters(name))
            if (name == "id") rejectSvgIdentifier(value)
            collectSvgLocalReferences(name, value)
            if (name == "d") {
                pathCommands += SVG_PATH_COMMAND.findAll(value).count()
                rejectSvgComplexity(pathCommands <= CREATION_SVG_MAXIMUM_PATH_COMMANDS)
            }
            if (name in SVG_GEOMETRY_ATTRIBUTES) {
                rejectSvgFiniteGeometry(value)
                geometryTokens += SVG_NUMBER.findAll(value).count()
                rejectSvgComplexity(geometryTokens <= CREATION_SVG_MAXIMUM_GEOMETRY_NUMBERS)
            }
        }
    }

    override fun endElement(uri: String?, localName: String?, qName: String?) {
        depth -= 1
    }

    override fun processingInstruction(target: String?, data: String?) {
        rejectSvgComplexity(false)
    }
}

private val XML_DECLARATION_ENCODING =
    Regex("""\bencoding\s*=\s*(['"])([^'"]+)\1""", RegexOption.IGNORE_CASE)

internal fun validateCreationSvgReferenceGraph(root: Element) {
    val state = SvgReferenceGraphState()
    collectSvgReferences(root, emptyList(), state)
    val visitStates = mutableMapOf<String, Int>()
    val heights = mutableMapOf<String, Int>()
    state.graph.keys.forEach {
        validateSvgReferenceDepth(it, state.graph, visitStates, heights, 0)
    }
}

internal data class CreationSvgExpansionCost(
    var elements: Long = 0,
    var rasterPixels: Long = 0,
    val uses: MutableList<String> = mutableListOf(),
)

internal fun validateCreationSvgReferenceExpansion(
    root: Element,
    rasterPixelsByElement: Map<Element, Long>,
) {
    val costs = mutableMapOf(
        CREATION_SVG_EXPANSION_ROOT to CreationSvgExpansionCost(),
    )

    fun collect(element: Element, ancestorIds: List<String>) {
        val id = element.getAttribute("id").takeIf(String::isNotBlank)
        if (id != null) costs.putIfAbsent(id, CreationSvgExpansionCost())
        val owners = if (id == null) ancestorIds else ancestorIds + id
        val targets = collectSvgLocalReferenceOccurrences(element)
        val pixels = rasterPixelsByElement[element] ?: 0L
        (listOf(CREATION_SVG_EXPANSION_ROOT) + owners).forEach { owner ->
            val cost = requireNotNull(costs[owner])
            cost.elements = Math.addExact(cost.elements, 1)
            cost.rasterPixels = Math.addExact(cost.rasterPixels, pixels)
            cost.uses += targets
            if (owner == CREATION_SVG_EXPANSION_ROOT) {
                rejectSvgComplexity(
                    cost.uses.size <= CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_EDGES,
                )
            }
        }
        var child = element.firstChild
        while (child != null) {
            if (child.nodeType == Node.ELEMENT_NODE) {
                collect(child as Element, owners)
            }
            child = child.nextSibling
        }
    }

    collect(root, emptyList())
    validateCreationSvgExpansionCosts(costs)
}

internal fun validateCreationSvgExpansionCosts(
    costs: Map<String, CreationSvgExpansionCost>,
) {
    rejectSvgComplexity(
        costs[CREATION_SVG_EXPANSION_ROOT].orEmptyUses().size <=
            CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_EDGES,
    )
    val visiting = mutableSetOf<String>()
    val memo = mutableMapOf<String, Pair<Long, Long>>()

    fun expand(id: String, depth: Int): Pair<Long, Long> {
        rejectSvgComplexity(depth <= CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_DEPTH)
        memo[id]?.let { return it }
        rejectSvgComplexity(visiting.add(id))
        val direct = costs[id]
        if (direct == null) {
            visiting.remove(id)
            return 0L to 0L
        }
        var elements = direct.elements
        var rasterPixels = direct.rasterPixels
        rejectSvgExpansionCost(elements, rasterPixels)
        direct.uses.forEach { target ->
            val expanded = expand(target, depth + 1)
            elements = Math.addExact(elements, expanded.first)
            rasterPixels = Math.addExact(rasterPixels, expanded.second)
            rejectSvgExpansionCost(elements, rasterPixels)
        }
        visiting.remove(id)
        return (elements to rasterPixels).also { memo[id] = it }
    }

    expand(CREATION_SVG_EXPANSION_ROOT, 0)
}

private fun CreationSvgExpansionCost?.orEmptyUses(): List<String> =
    this?.uses ?: emptyList()

private fun rejectSvgExpansionCost(elements: Long, rasterPixels: Long) {
    rejectSvgComplexity(
        elements <= CreationContract.MAXIMUM_SVG_ELEMENTS &&
            rasterPixels <= CreationContract.MAXIMUM_SVG_TOTAL_EMBEDDED_RASTER_PIXELS,
    )
}

private fun collectSvgReferences(
    element: Element,
    ancestorIds: List<String>,
    state: SvgReferenceGraphState,
) {
    val id = element.getAttribute("id")
        .takeIf(String::isNotBlank)
        ?.also(::rejectSvgIdentifier)
    val owners = if (id == null) {
        ancestorIds
    } else {
        rejectSvgComplexity(id !in state.graph)
        state.graph[id] = mutableSetOf()
        ancestorIds + id
    }
    for (index in 0 until element.attributes.length) {
        val attribute = element.attributes.item(index)
        val name = (attribute.localName ?: attribute.nodeName)
            .substringAfter(':')
            .lowercase(Locale.ROOT)
        val value = attribute.nodeValue.orEmpty()
        val references = collectSvgLocalReferences(name, value)
        owners.forEach { owner ->
            references.forEach { reference ->
                if (state.graph.getValue(owner).add(reference)) {
                    state.referenceEdges += 1
                    rejectSvgComplexity(
                        state.referenceEdges <= CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_EDGES,
                    )
                }
            }
        }
    }
    var child = element.firstChild
    while (child != null) {
        if (child.nodeType == Node.ELEMENT_NODE) {
            collectSvgReferences(child as Element, owners, state)
        }
        child = child.nextSibling
    }
}

private fun validateSvgReferenceDepth(
    id: String,
    graph: Map<String, Set<String>>,
    visitStates: MutableMap<String, Int>,
    heights: MutableMap<String, Int>,
    depth: Int,
): Int {
    val references = graph[id] ?: return 0
    rejectSvgComplexity(depth < CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_DEPTH)
    when (visitStates[id]) {
        1 -> rejectSvgComplexity(false)
        2 -> {
            val height = heights.getValue(id)
            rejectSvgComplexity(
                depth + height <= CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_DEPTH,
            )
            return height
        }
    }
    visitStates[id] = 1
    var height = 1
    references.forEach { reference ->
        val childHeight = validateSvgReferenceDepth(
            reference,
            graph,
            visitStates,
            heights,
            depth + 1,
        )
        height = maxOf(height, childHeight + 1)
        rejectSvgComplexity(
            depth + height <= CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_DEPTH,
        )
    }
    visitStates[id] = 2
    heights[id] = height
    return height
}

private fun collectSvgLocalReferences(name: String, value: String): Set<String> = buildSet {
    val trimmed = value.trim()
    if (name == "href" && trimmed.startsWith("#")) {
        add(trimmed.drop(1).also(::rejectSvgIdentifier))
    }
    SVG_URL_REFERENCE.findAll(value).forEach { match ->
        val target = match.groupValues[1].trim().trim('\'', '"')
        if (target.startsWith("#")) {
            add(target.drop(1).also(::rejectSvgIdentifier))
        }
    }
}

private fun collectSvgLocalReferenceOccurrences(element: Element): List<String> = buildList {
    var href: String? = null
    var xlinkHref: String? = null
    for (index in 0 until element.attributes.length) {
        val attribute = element.attributes.item(index)
        val name = attribute.nodeName
        val value = attribute.nodeValue.orEmpty()
        when (name) {
            "href" -> href = value
            "xlink:href" -> xlinkHref = value
        }
        SVG_URL_REFERENCE.findAll(value).forEach { match ->
            val target = match.groupValues[1].trim().trim('\'', '"')
            if (target.startsWith("#")) {
                add(target.drop(1).also(::rejectSvgIdentifier))
            }
        }
    }
    sequenceOf(href, xlinkHref)
        .filterNotNull()
        .firstOrNull(String::isNotBlank)
        ?.trim()
        ?.takeIf { it.startsWith("#") }
        ?.drop(1)
        ?.also(::rejectSvgIdentifier)
        ?.let(::add)
}

private data class SvgReferenceGraphState(
    val graph: MutableMap<String, MutableSet<String>> = mutableMapOf(),
    var referenceEdges: Int = 0,
)

private fun maximumSvgAttributeCharacters(name: String): Int = when (name) {
    "href" -> CreationContract.MAXIMUM_SVG_EMBEDDED_RASTER_CHARACTERS
    "d", "points" -> MAXIMUM_SVG_GEOMETRY_ATTRIBUTE_CHARACTERS
    "style" -> MAXIMUM_SVG_STYLE_CHARACTERS
    else -> MAXIMUM_SVG_ATTRIBUTE_CHARACTERS
}

private fun rejectSvgFiniteGeometry(value: String) {
    val lower = value.lowercase(Locale.ROOT)
    rejectSvgComplexity("nan" !in lower && "infinity" !in lower && "inf" !in lower)
    SVG_NUMBER.findAll(value).forEach { match ->
        val number = match.value.toDoubleOrNull()
        rejectSvgComplexity(
            number != null &&
                number.isFinite() &&
                kotlin.math.abs(number) <= CREATION_SVG_MAXIMUM_COORDINATE_MAGNITUDE,
        )
    }
}

private fun rejectSvgIdentifier(value: String) {
    rejectSvgComplexity(
        value.isNotEmpty() &&
            value.encodeToByteArray().size <= CREATION_SVG_MAXIMUM_LOCAL_IDENTIFIER_BYTES &&
            value.none {
                it.isWhitespace() ||
                    it.isISOControl() ||
                    it in setOf('\\', '%', '\'', '"', '(', ')')
            },
    )
}

private fun rejectSvgComplexity(accepted: Boolean) {
    if (!accepted) throw SAXException("The vector result is too complex")
}

private val SVG_NUMBER = Regex("""[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][-+]?\d+)?""")
private val SVG_PATH_COMMAND = Regex("[AaCcHhLlMmQqSsTtVvZz]")
private val SVG_URL_REFERENCE = Regex(
    """url\s*\(\s*([^)]*?)\s*\)""",
    RegexOption.IGNORE_CASE,
)
private val SVG_GEOMETRY_ATTRIBUTES = setOf(
    "d", "points", "viewbox", "transform",
    "x", "y", "x1", "y1", "x2", "y2",
    "cx", "cy", "r", "rx", "ry", "width", "height",
)
internal const val CREATION_SVG_EXPANSION_ROOT = "\u0000sgt-root"
internal const val CREATION_SVG_MAXIMUM_DOCUMENT_DEPTH = 128
internal const val CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_DEPTH = 64
internal const val CREATION_SVG_MAXIMUM_LOCAL_REFERENCE_EDGES = 100_000
internal const val CREATION_SVG_MAXIMUM_LOCAL_IDENTIFIER_BYTES = 512
internal const val CREATION_SVG_MAXIMUM_PATH_COMMANDS = 250_000
internal const val CREATION_SVG_MAXIMUM_GEOMETRY_NUMBERS = 1_000_000
internal const val CREATION_SVG_MAXIMUM_COORDINATE_MAGNITUDE = 10_000_000.0
private const val MAXIMUM_SVG_GEOMETRY_ATTRIBUTE_CHARACTERS = 262_144
private const val MAXIMUM_SVG_STYLE_CHARACTERS = 131_072
private const val MAXIMUM_SVG_ATTRIBUTE_CHARACTERS = 32_768

package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

class AccessibilityNodeResolverTest {
    @Test
    fun `exact path remains the preferred target identity`() {
        val target = Node("target")
        val root = Node("root", mutableListOf(Node("other"), target))

        val result = resolve(root, listOf(1), "target")

        assertSame(target, result.node)
        assertEquals(StableTargetResolutionKind.EXACT_PATH, result.kind)
    }

    @Test
    fun `changed child path recovers one exact fingerprint in the same tree`() {
        val target = Node("target")
        val root = Node(
            "root",
            mutableListOf(Node("container", mutableListOf(Node("other"), target))),
        )

        val result = resolve(root, listOf(0, 0), "target")

        assertSame(target, result.node)
        assertEquals(StableTargetResolutionKind.UNIQUE_FINGERPRINT, result.kind)
    }

    @Test
    fun `transient platform bounds failure retries through complete traversal`() {
        val target = Node("target")
        val root = Node("root", mutableListOf(target))
        var firstRead = true

        val result = resolveStableTarget(
            root = root,
            childPath = listOf(0),
            childCount = { it.children.size },
            childAt = { node, index ->
                if (node === root && firstRead) {
                    firstRead = false
                    throw IndexOutOfBoundsException("tree changed")
                }
                node.children[index]
            },
            matches = { it.identity == "target" },
        )

        assertSame(target, result.node)
        assertEquals(StableTargetResolutionKind.UNIQUE_FINGERPRINT, result.kind)
    }

    @Test
    fun `ambiguous fingerprint is never guessed`() {
        val root = Node("root", mutableListOf(Node("target"), Node("target")))

        val result = resolve(root, listOf(9), "target")

        assertNull(result.node)
        assertEquals(StableTargetResolutionKind.AMBIGUOUS, result.kind)
    }

    @Test
    fun `bounded incomplete traversal cannot claim a unique match`() {
        val root = Node("root", mutableListOf(Node("target")))

        val result = resolveStableTarget(
            root = root,
            childPath = listOf(9),
            maxNodes = 1,
            childCount = { it.children.size },
            childAt = { node, index -> node.children[index] },
            matches = { it.identity == "target" },
        )

        assertNull(result.node)
        assertEquals(StableTargetResolutionKind.INCOMPLETE, result.kind)
    }

    private fun resolve(
        root: Node,
        path: List<Int>,
        identity: String,
    ): StableTargetResolution<Node> = resolveStableTarget(
        root = root,
        childPath = path,
        childCount = { it.children.size },
        childAt = { node, index -> node.children[index] },
        matches = { it.identity == identity },
    )

    private data class Node(
        val identity: String,
        val children: MutableList<Node> = mutableListOf(),
    )
}

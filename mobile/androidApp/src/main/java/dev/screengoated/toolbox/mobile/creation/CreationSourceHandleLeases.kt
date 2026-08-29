package dev.screengoated.toolbox.mobile.creation

internal class CreationSourceHandleLeases {
    private val byOwner = mutableMapOf<String, Set<String>>()

    fun update(ownerId: String, paths: Set<String>): Set<String> {
        val previous = byOwner.put(ownerId, paths).orEmpty()
        if (paths.isEmpty()) byOwner.remove(ownerId)
        val retained = all()
        return previous - paths - retained
    }

    fun release(ownerId: String): Set<String> {
        val previous = byOwner.remove(ownerId).orEmpty()
        return previous - all()
    }

    fun paths(ownerId: String): Set<String> = byOwner[ownerId].orEmpty()

    fun all(): Set<String> = byOwner.values.flatten().toSet()
}

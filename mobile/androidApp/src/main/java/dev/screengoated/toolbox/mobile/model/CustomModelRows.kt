package dev.screengoated.toolbox.mobile.model

import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import java.util.Collections
import java.util.UUID

data class CustomModelRowIdentity(val epoch: String, val incarnation: Long)
data class CustomModelRow(val identity: CustomModelRowIdentity, val model: CustomPresetModelDefinition)

/** Owner-confined identities distinguish a row from a later row with the same model ID. */
internal class CustomModelRows(initial: List<CustomPresetModelDefinition>) {
    private val epoch = UUID.randomUUID().toString()
    private var nextIncarnation = 0L
    var rows: List<CustomModelRow> = emptyList()
        private set

    init { reconcile(initial) }

    fun reconcile(models: List<CustomPresetModelDefinition>, changedRow: CustomModelRowIdentity? = null) {
        val oldById = rows.groupBy { it.model.id }
        val newCounts = models.groupingBy { it.id }.eachCount()
        rows = Collections.unmodifiableList(models.map { model ->
            val old = oldById[model.id]?.singleOrNull()
            val retained = old?.takeIf {
                newCounts[model.id] == 1 && (it.model == model || it.identity == changedRow)
            }
            retained?.copy(model = model) ?: CustomModelRow(
                CustomModelRowIdentity(epoch, Math.addExact(nextIncarnation, 1).also { nextIncarnation = it }), model,
            )
        })
    }
}

package dev.drawkit.android.core

/** Rows in sheet order: top of the stack first, children under an open group. */
internal fun PaintState.layerRows(): List<LayerRow> {
    val rows = mutableListOf<LayerRow>()
    fun add(parent: Int?) {
        layers.filter { it.parent == parent }
            .sortedByDescending { it.index }
            .forEach { layer ->
                rows.add(layer)
                if (layer.isGroup && layer.expanded) add(layer.index)
            }
    }
    add(null)
    return rows
}

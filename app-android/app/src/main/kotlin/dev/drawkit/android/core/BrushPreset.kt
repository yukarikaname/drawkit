package dev.drawkit.android.core

/** The presets the core knows. The shell only names them; the stamp lives in Rust. */
enum class BrushPreset(val id: Int, val title: String) {
    SoftRound(0, "Soft round"),
    Pencil(1, "Pencil"),
    Ink(2, "Ink"),
    Marker(3, "Marker"),
    Airbrush(4, "Airbrush"),
    Bristle(5, "Bristle"),
    Spray(6, "Spray"),
    Chalk(7, "Chalk"),
    Watercolor(8, "Watercolor");

    companion object {
        fun fromId(id: Int): BrushPreset = entries.firstOrNull { it.id == id } ?: SoftRound
    }
}

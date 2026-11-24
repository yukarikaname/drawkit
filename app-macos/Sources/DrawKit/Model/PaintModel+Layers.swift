// The layer tree as the palette sees it.

import SwiftUI
import AppKit

extension PaintModel {
    // MARK: Layers

    func selectLayer(_ index: Int) {
        dk_layer_select(handle, UInt32(index))
        syncLayers()
    }

    /// Step through the raw layer indices: +1 is up towards the front, -1 is down.
    func stepLayer(_ delta: Int) {
        let next = activeLayer + delta
        guard layers.indices.contains(next) else { return }
        selectLayer(next)
        flash("Layer \(next + 1) · \(layers[next].name)")
    }

    func renameLayer(_ index: Int, name: String) {
        guard index < layers.count, !name.isEmpty else { return }
        let changed = name.withCString {
            dk_layer_set_name(handle, UInt32(index), $0)
        }
        guard changed != 0 else { return }
        edited()
        syncLayers()
    }

    /// Move a layer or group among its siblings. The whole subtree follows it.
    func moveLayer(_ index: Int, steps: Int) {
        guard steps != 0, index < layers.count else { return }
        guard dk_layer_move(handle, UInt32(index), Int32(steps)) != 0 else { return }
        edited()
        syncLayers()
        redrawAll()
        scheduleThumbnails()
    }

    func addLayer() {
        edited()
        _ = dk_layer_add(handle)
        syncLayers()
        redrawAll()
        scheduleThumbnails()
    }

    func removeLayer() {
        edited()
        _ = dk_layer_remove(handle)
        syncLayers()
        redrawAll()
        scheduleThumbnails()
    }

    func addGroup() {
        edited()
        _ = dk_layer_add_group(handle)
        syncLayers()
        redrawAll()
        scheduleThumbnails()
    }

    func addFillLayer(_ color: Color) {
        edited()
        let ink = Ink.from(color)
        _ = dk_layer_add_fill(handle, ink.red, ink.green, ink.blue, ink.alpha)
        syncLayers()
        redrawAll()
        scheduleThumbnails()
    }

    func depth(of layer: LayerInfo) -> Int {
        var depth = 0
        var parent = layer.parent
        while let group = parent.flatMap({ id in layers.first { $0.id == id } }) {
            depth += 1
            parent = group.parent
        }
        return depth
    }

    func setExpanded(index: Int, expanded: Bool) {
        dk_layer_set_expanded(handle, UInt32(index), expanded ? 1 : 0)
        syncLayers()
    }

    /// Put a premultiplied RGBA image above the active layer. Used by "bake reference"; the layer
    /// is not an undo step, which is why the shell names it and the painter can delete it.
    func addReferenceLayer(premultiplied: [UInt8], width: Int, height: Int) -> Bool {
        let added = premultiplied.withUnsafeBytes {
            dk_layer_add_image(handle, UInt32(width), UInt32(height),
                               $0.bindMemory(to: UInt8.self).baseAddress)
        }
        guard added != 0 else { return false }
        edited()
        syncLayers()
        redrawAll()
        refreshThumbnails()
        return true
    }

    func setVisible(index: Int, visible: Bool) {
        edited()
        dk_layer_set_visible(handle, UInt32(index), visible ? 1 : 0)
        syncLayers()
        afterLayerStateChange()
    }

    func setOpacity(index: Int, _ opacity: Double) {
        edited()
        dk_layer_set_opacity(handle, UInt32(index), Float(opacity))
        syncLayers()
        afterLayerStateChange()
    }

    func setClipping(index: Int, clipped: Bool) {
        dk_layer_set_clipping(handle, UInt32(index), clipped ? 1 : 0)
        syncLayers()
        gpuLayersDirty = true
        gpuSpecsDirty = true
        if gpuEligible {
            revision &+= 1
        } else {
            redrawAll()
        }
    }

    func setAlphaLocked(index: Int, locked: Bool) {
        dk_layer_set_alpha_locked(handle, UInt32(index), locked ? 1 : 0)
        syncLayers()
        revision &+= 1
    }

    func setTag(index: Int, tag: Int) {
        dk_layer_set_tag(handle, UInt32(index), Int32(tag))
        syncLayers()
        revision &+= 1
    }

    func setLocked(index: Int, locked: Bool) {
        dk_layer_set_locked(handle, UInt32(index), locked ? 1 : 0)
        syncLayers()
        revision &+= 1
    }

    func setBlend(index: Int, _ blend: BlendMode) {
        edited()
        dk_layer_set_blend(handle, UInt32(index), Int32(blend.rawValue))
        syncLayers()
        afterLayerStateChange()
    }

    func syncLayers() {
        gpuSpecsDirty = true
        let count = Int(dk_layer_count(handle))
        layers = (0..<count).map { index in
            var name = [UInt8](repeating: 0, count: 256)
            let capacity = UInt32(name.count)
            let written = name.withUnsafeMutableBufferPointer {
                dk_layer_name(handle, UInt32(index), $0.baseAddress, capacity)
            }
            let label = written > 0
                ? String(decoding: name.prefix(Int(written)), as: UTF8.self)
                : "Layer \(index + 1)"
            let parent = dk_layer_parent(handle, UInt32(index))
            return LayerInfo(
                id: index,
                name: label,
                visible: dk_layer_visible(handle, UInt32(index)) != 0,
                opacity: Double(dk_layer_opacity(handle, UInt32(index))),
                kind: LayerKind(rawValue: Int(dk_layer_kind(handle, UInt32(index)))) ?? .paint,
                blend: BlendMode(rawValue: Int(dk_layer_blend(handle, UInt32(index)))) ?? .normal,
                locked: dk_layer_locked(handle, UInt32(index)) != 0,
                alphaLocked: dk_layer_alpha_locked(handle, UInt32(index)) != 0,
                clipping: dk_layer_clipping(handle, UInt32(index)) != 0,
                parent: parent < 0 ? nil : Int(parent),
                expanded: dk_layer_expanded(handle, UInt32(index)) != 0,
                colorTag: Int(dk_layer_tag(handle, UInt32(index)))
            )
        }
        activeLayer = Int(dk_layer_active(handle))
    }
}

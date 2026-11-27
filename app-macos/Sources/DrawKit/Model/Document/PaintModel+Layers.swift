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
        flash(String(localized: "Layer \(next + 1) · \(layers[next].name)"))
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

    /// Move a layer or group among its siblings. The whole subtree follows it. A plain paint
    /// layer with no children moves its resident host in place; anything else rebuilds the stack.
    func moveLayer(_ index: Int, steps: Int) {
        guard steps != 0, index < layers.count else { return }
        let hasChildren = layers.contains { $0.parent == layers[index].id }
        let target = index + steps
        guard dk_layer_move(handle, UInt32(index), Int32(steps)) != 0 else { return }
        edited()
        syncLayers()
        if !hasChildren, layers.indices.contains(target),
           let surface = paintSurface,
           surface.moveResidentLayer(from: index, to: target) {
            requestPresent?()
        } else {
            redrawAll()
        }
        scheduleThumbnails()
    }

    /// Reparent a layer or group into a group, or out to the root. Returns the new index.
    @discardableResult
    func reparentLayer(_ index: Int, parent: Int?) -> Int? {
        let result = dk_layer_set_parent(handle, UInt32(index), parent.map(Int32.init) ?? -1)
        guard result >= 0 else { return nil }
        dk_layer_select(handle, UInt32(result))
        edited()
        syncLayers()
        redrawAll()
        scheduleThumbnails()
        return Int(result)
    }

    /// Top-down tree rows: a group header, then its children when expanded.
    func paletteRows() -> [LayerInfo] {
        var result: [LayerInfo] = []
        appendPaletteChildren(of: nil, to: &result)
        return result
    }

    private func appendPaletteChildren(of parent: Int?, to result: inout [LayerInfo]) {
        let children = layers.filter { $0.parent == parent }
        for layer in children.reversed() {
            result.append(layer)
            if layer.kind == .group && layer.expanded {
                appendPaletteChildren(of: layer.id, to: &result)
            }
        }
    }

    func addLayer() {
        edited()
        let index = Int(dk_layer_add(handle))
        syncLayers()
        // A new paint layer is empty, so the resident stack only needs a host, not an upload.
        if layers.indices.contains(index), let surface = paintSurface,
           surface.insertResidentLayer(index: index, width: width, height: height) {
            requestPresent?()
        } else {
            redrawAll()
        }
        scheduleThumbnails()
    }

    func removeLayer() {
        let index = activeLayer
        edited()
        guard dk_layer_remove(handle) != 0 else { return }
        syncLayers()
        // A group or clip reference above the removed layer makes the stack refuse; the core has
        // already applied the removal, so rebuild rather than leave the two out of step.
        if let surface = paintSurface, surface.removeResidentLayer(index: index) {
            requestPresent?()
        } else {
            redrawAll()
        }
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

    /// Add a canvas-sized premultiplied RGBA image as a new layer. The core records the insertion,
    /// so one undo removes it again.
    func addImageLayer(premultiplied: [UInt8], name: String) -> Bool {
        guard premultiplied.count >= width * height * 4 else { return false }
        let added = premultiplied.withUnsafeBytes {
            dk_layer_add_image(handle, UInt32(width), UInt32(height),
                               $0.bindMemory(to: UInt8.self).baseAddress)
        }
        guard added != 0 else { return false }
        isBlank = false
        renameLayer(activeLayer, name: name)
        edited()
        syncLayers()
        // The pasted pixels are a new layer; insert its host and upload that one layer instead of
        // re-uploading the whole stack.
        if let surface = paintSurface,
           surface.insertResidentLayer(index: Int(added), width: width, height: height) {
            _ = uploadLayerRect(surface, index: Int(added), x: 0, y: 0,
                                width: width, height: height)
            requestPresent?()
        } else {
            redrawAll()
        }
        scheduleThumbnails()
        return true
    }

    /// Add an image from Services or a drop as a new layer. A paste keeps its 1:1 size because it
    /// is a copy of pixels that must not move; an import is usually a photo larger than the sheet,
    /// so it is scaled down to fit and centred. Small images keep their size.
    @discardableResult
    func addImageLayer(fitted source: CGImage, name: String) -> Bool {
        let sourceWidth = source.width
        let sourceHeight = source.height
        guard sourceWidth > 0, sourceHeight > 0, width > 0, height > 0 else { return false }
        let scale = min(1.0, min(Double(width) / Double(sourceWidth),
                                 Double(height) / Double(sourceHeight)))
        let drawWidth = max(1, Int((Double(sourceWidth) * scale).rounded()))
        let drawHeight = max(1, Int((Double(sourceHeight) * scale).rounded()))
        let originX = (width - drawWidth) / 2
        let originY = (height - drawHeight) / 2
        var buffer = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = buffer.withUnsafeMutableBytes { bytes -> Bool in
            guard let context = CGContext(
                data: bytes.baseAddress, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: colorSpace.cgColorSpace,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
            context.interpolationQuality = .high
            // Layer rows run top down; a bitmap context's origin is bottom left.
            context.draw(source, in: CGRect(x: originX, y: height - originY - drawHeight,
                                            width: drawWidth, height: drawHeight))
            return true
        }
        guard drawn else { return false }
        return addImageLayer(premultiplied: buffer, name: name)
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

    func setReference(index: Int, reference: Bool) {
        dk_layer_set_reference(handle, UInt32(index), reference ? 1 : 0)
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
                colorTag: Int(dk_layer_tag(handle, UInt32(index))),
                reference: dk_layer_reference(handle, UInt32(index)) != 0
            )
        }
        activeLayer = Int(dk_layer_active(handle))
    }
}

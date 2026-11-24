// The iPad layer tree. Groups and fill layers come from the same core as the desktop; this file is
// the sheet's data: rows with a depth, a folder or fill marker, and the actions beside them.

import SwiftUI
import UIKit

struct IOSLayer: Identifiable, Equatable {
    let id: Int
    let name: String
    let visible: Bool
    let opacity: Double
    /// 0 paint, 1 group, 2 fill.
    let kind: Int
    let parent: Int?
    let expanded: Bool
    let depth: Int
    let tag: Int

    var isGroup: Bool { kind == 1 }
    var isFill: Bool { kind == 2 }
}

extension Studio {
    func addLayer() {
        _ = dk_layer_add(handle)
        refreshAll()
        refreshLayers()
        finishChange()
    }

    func addGroup() {
        _ = dk_layer_add_group(handle)
        refreshAll()
        refreshLayers()
        finishChange()
    }

    func addFillLayer() {
        let color = colorComponents()
        _ = dk_layer_add_fill(handle, color.0, color.1, color.2, Float(opacity))
        refreshAll()
        refreshLayers()
        finishChange()
    }

    func moveLayer(_ index: Int, steps: Int32) {
        guard dk_layer_move(handle, UInt32(index), steps) != 0 else { return }
        refreshAll()
        refreshLayers()
        finishChange()
    }

    func removeLayer() {
        guard dk_layer_remove(handle) != 0 else { return }
        refreshAll()
        refreshLayers()
        finishChange()
    }

    func selectLayer(_ index: Int) {
        dk_layer_select(handle, UInt32(index))
        refreshLayers()
        revision += 1
    }

    func setVisible(_ index: Int, _ visible: Bool) {
        dk_layer_set_visible(handle, UInt32(index), visible ? 1 : 0)
        refreshAll()
        refreshLayers()
        finishChange()
    }

    func setOpacity(_ index: Int, _ value: Double) {
        dk_layer_set_opacity(handle, UInt32(index), Float(value))
        refreshAll()
        refreshLayers()
        revision += 1
    }

    /// Collapse or open a group. This changes only the sheet, not a pixel.
    func setTag(_ index: Int, tag: Int) {
        dk_layer_set_tag(handle, UInt32(index), Int32(tag))
        refreshLayers()
        revision += 1
    }

    func renameLayer(_ index: Int, name: String) {
        guard !name.isEmpty else { return }
        _ = name.withCString { dk_layer_set_name(handle, UInt32(index), $0) }
        refreshLayers()
    }

    func setExpanded(_ index: Int, _ expanded: Bool) {
        dk_layer_set_expanded(handle, UInt32(index), expanded ? 1 : 0)
        refreshLayers()
    }

    func refreshLayers() {
        let count = Int(dk_layer_count(handle))
        activeLayer = Int(dk_layer_active(handle))
        let parents = (0..<count).map { Int(dk_layer_parent(handle, UInt32($0))) }
        layers = (0..<count).map { index in
            var name = [UInt8](repeating: 0, count: 64)
            let written = name.withUnsafeMutableBufferPointer {
                dk_layer_name(handle, UInt32(index), $0.baseAddress, UInt32($0.count))
            }
            let label = written > 0
                ? String(decoding: name.prefix(Int(written)), as: UTF8.self)
                : "Layer \(index + 1)"
            return IOSLayer(
                id: index,
                name: label,
                visible: dk_layer_visible(handle, UInt32(index)) != 0,
                opacity: Double(dk_layer_opacity(handle, UInt32(index))),
                kind: Int(dk_layer_kind(handle, UInt32(index))),
                parent: parents[index] < 0 ? nil : parents[index],
                expanded: dk_layer_expanded(handle, UInt32(index)) != 0,
                depth: depth(of: index, parents: parents),
                tag: Int(dk_layer_tag(handle, UInt32(index)))
            )
        }
        canUndo = dk_can_undo(handle) != 0
        canRedo = dk_can_redo(handle) != 0
        isBlank = dk_is_clear(handle) != 0
    }

    private func depth(of index: Int, parents: [Int]) -> Int {
        var depth = 0
        var parent = parents[index]
        while parent >= 0 && parent < parents.count {
            depth += 1
            parent = parents[parent]
        }
        return depth
    }

    /// Rows in sheet order: top of the stack first, children under an open group.
    func layerRows() -> [IOSLayer] {
        var rows: [IOSLayer] = []
        func add(parent: Int?) {
            let siblings = layers.filter { $0.parent == parent }.sorted { $0.id > $1.id }
            for layer in siblings {
                rows.append(layer)
                if layer.isGroup && layer.expanded {
                    add(parent: layer.id)
                }
            }
        }
        add(parent: nil)
        return rows
    }

    func thumbnail(_ index: Int, size: Int = 52) -> UIImage? {
        var bytes = [UInt8](repeating: 0, count: size * size * 4)
        let drawn = bytes.withUnsafeMutableBufferPointer {
            dk_layer_thumbnail(handle, UInt32(index), UInt32(size), UInt32(size), $0.baseAddress)
        }
        guard drawn != 0 else { return nil }
        let data = Data(bytes) as CFData
        guard let provider = CGDataProvider(data: data),
              let image = CGImage(
                width: size, height: size, bitsPerComponent: 8, bitsPerPixel: 32,
                bytesPerRow: size * 4, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent
              ) else { return nil }
        return UIImage(cgImage: image)
    }
}

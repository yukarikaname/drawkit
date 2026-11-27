// Experimental history branches: list, switch, and forget the dormant alternatives.

import Foundation

struct HistoryBranchInfo: Identifiable, Equatable {
    let id: UInt64
    let parent: UInt64?
    let current: Bool
    let bytes: UInt64
}

@_silgen_name("dk_set_history_branches")
func dk_set_history_branches(_ handle: UnsafeMutableRawPointer?, _ enabled: Int32)
@_silgen_name("dk_history_branches_enabled")
func dk_history_branches_enabled(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_history_branch_count")
func dk_history_branch_count(_ handle: UnsafeMutableRawPointer?) -> UInt32
@_silgen_name("dk_history_branch")
func dk_history_branch(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32,
    _ out: UnsafeMutablePointer<UInt64>?
) -> Int32
@_silgen_name("dk_history_branch_current")
func dk_history_branch_current(_ handle: UnsafeMutableRawPointer?) -> Int64
@_silgen_name("dk_history_branch_pending")
func dk_history_branch_pending(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_history_branch_switch")
func dk_history_branch_switch(_ handle: UnsafeMutableRawPointer?, _ id: UInt64) -> Int32
@_silgen_name("dk_history_branch_clear")
func dk_history_branch_clear(_ handle: UnsafeMutableRawPointer?)

extension PaintModel {
    func setHistoryBranches(_ enabled: Bool) {
        guard let handle else { return }
        dk_set_history_branches(handle, enabled ? 1 : 0)
        refreshHistoryBranches()
    }

    /// Read the dormant branches from the core. Cheap: at most a handful of rows.
    func refreshHistoryBranches() {
        guard let handle, dk_history_branches_enabled(handle) != 0 else {
            historyBranches = []
            branchPending = false
            return
        }
        var rows: [HistoryBranchInfo] = []
        let count = dk_history_branch_count(handle)
        for index in 0..<count {
            var values = [UInt64](repeating: 0, count: 4)
            let read = values.withUnsafeMutableBufferPointer {
                dk_history_branch(handle, index, $0.baseAddress)
            }
            guard read != 0 else { continue }
            rows.append(
                HistoryBranchInfo(
                    id: values[0],
                    parent: values[1] == .max ? nil : values[1],
                    current: values[2] != 0,
                    bytes: values[3]))
        }
        historyBranches = rows
        branchPending = dk_history_branch_pending(handle) != 0
    }

    func clearHistoryBranches() {
        guard let handle else { return }
        dk_history_branch_clear(handle)
        refreshHistoryBranches()
        revision &+= 1
    }

    /// Replace the live document with a dormant branch. The GPU tile journal and the replay cache
    /// belong to the old path, so both are dropped and the layer stack is uploaded again.
    @discardableResult
    func switchHistoryBranch(_ id: UInt64) -> Bool {
        guard let handle, dk_history_branch_switch(handle, id) != 0 else { return false }
        previewMs = nil
        tapeReplay = nil
        tapeReplayUnavailable = false
        paintSurface?.clearTileHistory()
        gpuStale = false
        syncLayers()
        markGPURebuild()
        refreshHistoryBranches()
        refreshThumbnails()
        canUndo = dk_can_undo(handle) != 0
        canRedo = dk_can_redo(handle) != 0
        revision &+= 1
        return true
    }
}

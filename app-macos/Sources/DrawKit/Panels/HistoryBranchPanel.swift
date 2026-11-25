// The experimental branching history, shown as a horizontal tree when a branch exists.

import SwiftUI

struct HistoryBranchPanel: View {
    let model: PaintModel

    private func depth(of branch: HistoryBranchInfo) -> Int {
        var depth = 0
        var parent = branch.parent
        while let id = parent,
              let next = model.historyBranches.first(where: { $0.id == id }),
              depth < 8 {
            depth += 1
            parent = next.parent
        }
        return depth
    }

    private var columns: [[HistoryBranchInfo]] {
        let deepest = model.historyBranches.map(depth).max() ?? 0
        return (0...deepest).map { level in
            model.historyBranches.filter { depth(of: $0) == level }
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Label("History branches", systemImage: "arrow.triangle.branch")
                    .font(.headline)
                Spacer()
                if !model.historyBranches.isEmpty {
                    Button("Forget") { model.clearHistoryBranches() }
                        .buttonStyle(.borderless)
                        .foregroundStyle(.secondary)
                }
            }
            if model.branchPending {
                Label("Draw after an undo to keep the abandoned path", systemImage: "clock.arrow.circlepath")
                    .font(.caption)
                    .foregroundStyle(Brand.vermilion)
            }
            if model.historyBranches.isEmpty {
                Text("Undo a stroke, then draw a different one. The path you left becomes a branch.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            } else {
                ScrollView([.horizontal, .vertical]) {
                    HStack(alignment: .top, spacing: 22) {
                        ForEach(Array(columns.enumerated()), id: \.offset) { _, column in
                            VStack(alignment: .leading, spacing: 8) {
                                ForEach(column) { branch in
                                    branchButton(branch)
                                }
                            }
                        }
                    }
                    .padding(.vertical, 2)
                }
                .frame(maxWidth: 520, maxHeight: 260)
            }
        }
        .padding(14)
        .frame(width: 440)
    }

    private func branchButton(_ branch: HistoryBranchInfo) -> some View {
        Button {
            model.switchHistoryBranch(branch.id)
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text("Branch \(branch.id)")
                        .font(.callout.weight(branch.current ? .semibold : .regular))
                    if branch.current {
                        Image(systemName: "smallcircle.filled.circle")
                            .foregroundStyle(Brand.vermilion)
                    }
                }
                Text(byteCount(branch.bytes))
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .frame(minWidth: 108, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(branch.current ? Brand.vermilion.opacity(0.14) : Brand.carbon)
                    .overlay(RoundedRectangle(cornerRadius: 8)
                        .stroke(branch.current ? Brand.vermilion.opacity(0.5)
                                               : .white.opacity(0.08), lineWidth: 1)))
        }
        .buttonStyle(.plain)
        .help(branch.current ? "This is the live branch" : "Switch to this branch")
    }

    private func byteCount(_ bytes: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .memory)
    }
}

import DesignSystem
import SwiftUI

/// After the merge: keep the worktree, or delete it and keep its branch.
@MainActor
struct MergedTreeSection: View {
    let cleanup: WorktreeCleanup
    let offline: Bool
    let keep: () -> Void
    let delete: () -> Void

    var body: some View {
        Section {
            Text("Delete the worktree \(cleanup.name)? \(cleanup.closes)").foregroundStyle(Palette.text2.color)
            HStack(spacing: Space.s) {
                Button("Keep", action: keep)
                    .buttonStyle(.bordered)
                Button(role: .destructive, action: delete) {
                    Label("Delete Worktree", systemImage: "trash").frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                // The app's accent tint would otherwise beat the role: white on near-white in dark mode.
                .tint(Palette.failed.color)
                .disabled(offline)
            }
            .controlSize(.large)
        }
    }
}

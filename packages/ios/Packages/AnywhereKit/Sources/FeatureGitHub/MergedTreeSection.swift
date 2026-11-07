import DesignSystem
import SwiftUI

/// After the merge: keep the worktree, or delete it and keep its branch.
@MainActor
struct MergedTreeSection: View {
    let cleanup: WorktreeCleanup
    let deleting: Bool
    let offline: Bool
    let keep: () -> Void
    let delete: () -> Void

    var body: some View {
        Section {
            Text("Delete the worktree \(cleanup.name)? \(cleanup.closes)").foregroundStyle(Palette.text2.color)
            HStack(spacing: Space.s) {
                Button(action: keep) { Text("Keep").frame(maxWidth: .infinity) }
                    .buttonStyle(.bordered)
                Button(role: .destructive, action: delete) {
                    Label(deleting ? "Deleting…" : "Delete Worktree", systemImage: "trash").frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(deleting || offline)
            }
            .controlSize(.large)
        }
    }
}

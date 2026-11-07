import DesignSystem
import Stores
import SwiftUI

/// A worktree's or Local's swipes and menu: a new session in it on the leading edge, Delete on the trailing one. Delete
/// always asks first, so it never takes a full swipe, and the project's own checkout has none.
@MainActor
struct TreeActions: ViewModifier {
    let tree: HomeList.Tree
    let project: String
    let canManage: Bool
    @Binding var making: Making?
    @Binding var action: RowAction?

    func body(content: Content) -> some View {
        content
            .swipeActions(edge: .leading) {
                Button { making = .session(project: project, place: TreeMenu.place(tree)) } label: {
                    Label("New Session", systemImage: "square.and.pencil")
                }
                .tint(Palette.accent.color)
            }
            .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                if canManage && tree.kind != .own {
                    // No destructive role: that slides the row out before the confirm.
                    Button { action = .delete(tree) } label: { Label("Delete", systemImage: "trash") }.tint(Palette.failed.color)
                }
            }
            .contextMenu { TreeMenu(tree: tree, project: project, canManage: canManage, making: $making, action: $action) }
    }
}

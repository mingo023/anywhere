import ChatModel
import DesignSystem
import Stores
import SwiftUI

extension View {
    /// A title over a small status line, centred in the bar on every OS, as a messenger names a conversation. iOS 26's
    /// navigation subtitle sets the title at the leading edge beside Back instead. `git` is the tree's status, which takes
    /// the place of `subtitle` once it is read.
    func barTitle(_ title: String, subtitle: String, git: TreeChanges? = nil) -> some View {
        navigationTitle(title).toolbar {
            ToolbarItem(placement: .principal) { BarTitle(title: title, subtitle: subtitle, git: git) }
        }
    }
}

/// Its own view and glass: a status read redraws only this, and the transcript scrolling under stays off the words.
@MainActor
private struct BarTitle: View {
    let title: String
    let subtitle: String
    let git: TreeChanges?

    var body: some View {
        VStack(spacing: 1) {
            Text(title).font(.system(size: 16, weight: .semibold)).foregroundStyle(Palette.text.color)
            detail.font(.system(size: 12)).foregroundStyle(Palette.text3.color)
        }
        .lineLimit(1)
        .padding(.horizontal, Space.l)
        .frame(minHeight: HitTarget.minimum)
        .glass(in: Capsule())
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private var detail: some View {
        if let head = git?.head {
            HStack(spacing: Space.xs) {
                Text(ChatTitle.branch(head.branch, detached: head.detached)).fontDesign(.monospaced).truncationMode(.middle)
                if let change = ChatTitle.change(files: head.totals.files, added: head.totals.added, removed: head.totals.removed) {
                    Text(change).monospacedDigit().fixedSize()
                }
            }
        } else {
            Text(subtitle)
        }
    }
}

import ChatModel
import DesignSystem
import Stores
import SwiftUI

/// One entry of the bar's menu: a screen the chat's tree opens.
struct ChatMenuItem: Identifiable {
    let title: String
    let icon: Lucide
    let run: () -> Void

    var id: String { title }
}

/// The chat's bar as the redesign draws it: a round glass Back, the title over the tree's branch and changes in a glass
/// capsule, and a round glass menu. It stands in for the navigation bar, which the screen hides.
@MainActor
struct ChatTopBar: View {
    let title: String
    let subtitle: String
    let git: TreeChanges?
    let menu: [ChatMenuItem]
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        HStack(spacing: Space.s) {
            Button { dismiss() } label: { LucideIcon(.chevronLeft, size: 20) }
                .buttonStyle(RoundButtonStyle())
                .accessibilityLabel("Back")
            VStack(spacing: 1) {
                Text(title).font(.system(size: 15, weight: .semibold)).foregroundStyle(Palette.text.color)
                BarDetail(subtitle: subtitle, git: git)
            }
            .lineLimit(1)
            .padding(.horizontal, Space.l)
            .frame(maxWidth: .infinity)
            .frame(height: 48)
            .background(Palette.glass.color, in: Capsule())
            .overlay(Rim().fill(Palette.glassBorder.color, style: FillStyle(eoFill: true)))
            .accessibilityElement(children: .combine)
            if menu.isEmpty {
                Color.clear.frame(width: 44, height: 44)
            } else {
                Menu {
                    ForEach(menu) { item in
                        Button { item.run() } label: { Label(item.title, lucide: item.icon) }
                    }
                } label: {
                    LucideIcon(.ellipsis, size: 20)
                        .foregroundStyle(Palette.text.color)
                        .frame(width: 44, height: 44)
                        .background(Palette.glass.color, in: Circle())
                        .overlay(Circle().strokeBorder(Palette.glassBorder.color, lineWidth: 0.5))
                        .contentShape(Circle())
                }
                .menuStyle(.button)
                .menuIndicator(.hidden)
                .buttonStyle(.plain)
                .accessibilityLabel("More")
            }
        }
        .padding(.horizontal, Inset.margin)
    }
}

/// A capsule's half-point edge, filled between two outlines: a stroke along a capsule's ends draws a stray line on macOS.
private struct Rim: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Capsule().path(in: rect)
        path.addPath(Capsule().path(in: rect.insetBy(dx: 0.5, dy: 0.5)))
        return path
    }
}

/// Its own view: a status read redraws only this.
@MainActor
private struct BarDetail: View {
    let subtitle: String
    let git: TreeChanges?

    var body: some View {
        if let head = git?.head {
            HStack(spacing: Space.s - 2) {
                Text(ChatTitle.branch(head.branch, detached: head.detached)).truncationMode(.middle)
                let totals = head.totals
                if let change = ChatTitle.change(files: totals.files, added: totals.added, removed: totals.removed) {
                    if totals.added + totals.removed > 0 {
                        Text("+\(totals.added)").foregroundStyle(Palette.successText.color).fixedSize()
                        Text("−\(totals.removed)").foregroundStyle(Palette.failedText.color).fixedSize()
                    } else {
                        Text(change).fixedSize()
                    }
                }
            }
            .font(.system(size: 11, design: .monospaced)).monospacedDigit()
            .foregroundStyle(Palette.text2.color)
        } else {
            Text(subtitle).font(.system(size: 11)).foregroundStyle(Palette.text2.color)
        }
    }
}

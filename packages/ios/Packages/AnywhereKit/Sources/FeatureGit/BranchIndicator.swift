import DesignSystem
import Stores
import SwiftUI

/// The branch a tree is on, how far it is from its base, and what it grew from.
@MainActor
struct BranchIndicator: View {
    let head: GitHead

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            HStack(spacing: Space.s) {
                Image(systemName: head.detached ? "circle.dashed" : "arrow.triangle.branch")
                    .foregroundStyle(Palette.graphHead.color)
                Text(head.detached ? "Detached HEAD" : head.branch ?? "No branch")
                    .font(Typography.headline)
                    .foregroundStyle(Palette.text.color)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let ab = GitText.aheadBehind(head) {
                    Text(ab).font(Typography.digits).foregroundStyle(Palette.text2.color)
                }
                Spacer(minLength: 0)
            }
            if let base = head.base {
                Text("from \(base)")
                    .font(Typography.caption)
                    .foregroundStyle(Palette.text3.color)
                    .lineLimit(1)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

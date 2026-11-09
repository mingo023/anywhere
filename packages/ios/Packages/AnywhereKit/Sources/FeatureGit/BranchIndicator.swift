import DesignSystem
import Stores
import SwiftUI

/// The branch a tree is on and the base it grew from, then how far apart they are.
@MainActor
struct BranchIndicator: View {
    let head: GitHead

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            HStack(spacing: Space.s) {
                Image.named(head.detached ? "circle.dashed" : "branch", size: 17)
                    .foregroundStyle(Palette.graphHead.color)
                Text(GitText.branchLine(head))
                    .font(Typography.headline)
                    .foregroundStyle(Palette.text.color)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 0)
            }
            if let distance = GitText.distance(head) {
                Text(distance)
                    .font(Typography.caption)
                    .monospacedDigit()
                    .foregroundStyle(Palette.text3.color)
                    .lineLimit(1)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

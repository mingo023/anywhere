import DesignSystem
import SwiftUI

/// How much of each pill the composer's bar has room for: everything; then without the effort, the goal's words and
/// the agents chip's; then without the access's; then the leading marks alone.
enum PillFit: Comparable {
    case full, short, compact, icons
}

/// A setting in the composer's bar: a 34 pt capsule of its words, or of its mark alone when short of room.
@MainActor
struct PillLabel<Content: View>: View {
    var fill: Token?
    @ViewBuilder let content: () -> Content

    var body: some View {
        HStack(spacing: 5, content: content)
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(Palette.textBody.color)
            .lineLimit(1)
            .padding(.horizontal, 12)
            .frame(height: 34)
            .background((fill ?? Palette.fill3).color, in: Capsule())
            .hitTarget()
    }
}

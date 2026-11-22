import DesignSystem
import SwiftUI

/// How much of each pill the composer's bar has room for: everything; then without the effort, the goal's words and
/// the agents chip's; then without the access's; then the leading marks alone.
enum PillFit: Comparable {
    case full, short, compact, icons
}

/// A setting in the composer's bar, as the desktop's pill: what leads it, its words, and a chevron while it opens a menu.
@MainActor
struct PillLabel<Content: View>: View {
    let opens: Bool
    var fill: Token?
    @ViewBuilder let content: () -> Content

    var body: some View {
        HStack(spacing: 5) {
            content()
            if opens { Image(systemName: "chevron.down").font(.system(size: 9, weight: .semibold)).foregroundStyle(Palette.text4.color) }
        }
        .font(ChatFont.caption.weight(.medium))
        .lineLimit(1)
        .padding(.horizontal, 10)
        .frame(height: 30)
        .background((fill ?? Palette.fill2).color, in: Capsule())
        .hitTarget()
    }
}

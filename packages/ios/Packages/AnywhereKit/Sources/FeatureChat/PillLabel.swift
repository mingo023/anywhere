import DesignSystem
import SwiftUI

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
        .padding(.horizontal, 8)
        .frame(height: 30)
        .background((fill ?? .fixed(0)).color, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
        .hitTarget()
    }
}

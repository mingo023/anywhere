import DesignSystem
import SwiftUI

/// "↓ Scroll to bottom", floating just above the composer while the end is far.
@MainActor
struct JumpPill: View {
    let action: () -> Void

    var body: some View {
        Button {
            Haptics.tap()
            action()
        } label: {
            HStack(spacing: 6) {
                Text("↓").foregroundStyle(Palette.text2.color)
                Text("Scroll to bottom").foregroundStyle(Palette.text.color)
            }
            .font(ChatFont.small.weight(.medium))
            .padding(.horizontal, 14)
            .frame(height: 32)
            .background(Palette.popover.color, in: Capsule())
            .overlay(Capsule().strokeBorder(Palette.hairline.color))
            .shadow(color: .black.opacity(0.12), radius: 8, y: 2)
        }
        .buttonStyle(.plain)
        .transition(.opacity.combined(with: .offset(y: 2)))
    }
}

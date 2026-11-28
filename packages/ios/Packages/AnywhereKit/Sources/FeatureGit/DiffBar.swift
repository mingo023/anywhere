import DesignSystem
import SwiftUI

/// Under a diff: the previous file, the next change with where the file sits among those it steps through, and the next file.
@MainActor
struct DiffBar: View {
    let previous: (() -> Void)?
    let next: (() -> Void)?
    /// "File 1 of 4".
    let place: String
    /// The next change, or the next file's first once this file has none left; nil at the very end.
    let step: (() -> Void)?

    var body: some View {
        HStack(spacing: Space.s) {
            round(.chevronLeft, "Previous File", previous)
            Spacer(minLength: 0)
            Button { step?() } label: {
                HStack(spacing: 6) {
                    LucideIcon(.arrowDown, size: 17)
                    Text("Next change").fontWeight(.semibold)
                    Text(place).foregroundStyle(Palette.text2.color).padding(.leading, Space.xs)
                }
                .font(.system(size: 15))
                .foregroundStyle(Palette.text.color)
                .lineLimit(1)
                .padding(.horizontal, 18)
                .frame(height: 48)
                .background(Palette.glass.color, in: Capsule())
                .overlay(Capsule().strokeBorder(Palette.glassBorder.color, lineWidth: 0.5))
                .contentShape(Capsule())
            }
            .buttonStyle(.plain)
            .disabled(step == nil)
            .opacity(step == nil ? 0.45 : 1)
            Spacer(minLength: 0)
            round(.chevronRight, "Next File", next)
        }
        .padding(.horizontal, Space.l)
        .padding(.top, Space.m)
        .padding(.bottom, Space.xs)
        .background(LinearGradient(colors: [Palette.background.color.opacity(0), Palette.background.color], startPoint: .top, endPoint: .init(x: 0.5, y: 0.45)))
    }

    private func round(_ icon: Lucide, _ label: String, _ action: (() -> Void)?) -> some View {
        Button { action?() } label: { LucideIcon(icon, size: 20) }
            .buttonStyle(RoundButtonStyle(size: 48))
            .disabled(action == nil)
            .opacity(action == nil ? 0.45 : 1)
            .accessibilityLabel(label)
    }
}

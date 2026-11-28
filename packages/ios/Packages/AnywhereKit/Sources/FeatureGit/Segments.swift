import DesignSystem
import Stores
import SwiftUI

/// The design's two-way segmented control: a 36 pt capsule of the quiet fill, the picked half raised on it.
@MainActor
struct Segments<Value: Hashable>: View {
    let options: [(value: Value, title: String)]
    @Binding var selection: Value

    var body: some View {
        HStack(spacing: 0) {
            ForEach(options, id: \.value) { option in
                let picked = option.value == selection
                Button { selection = option.value } label: {
                    Text(option.title)
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(Palette.text.color)
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background { if picked { Capsule().fill(Palette.segment.color).shadow(color: .black.opacity(0.18), radius: 1.5, y: 1) } }
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(picked ? .isSelected : [])
            }
        }
        .padding(2)
        .frame(height: 36)
        .background(Palette.fill3.color, in: Capsule())
        .padding(.horizontal, Inset.margin)
    }
}

/// A 20 pt circle that fills with ink and a check once on: a file picked, or marked as viewed.
@MainActor
struct CheckCircle: View {
    let on: Bool

    var body: some View {
        ZStack {
            Circle().strokeBorder(on ? Palette.text.color : Palette.text4.color, lineWidth: 1.5)
            if on {
                Circle().fill(Palette.text.color)
                LucideIcon(.check, size: 12, weight: 2.5).foregroundStyle(Palette.onText.color)
            }
        }
        .frame(width: 20, height: 20)
    }
}

/// Dims what a screen kept from a Mac that has gone, once it is more than a blip.
@MainActor
struct DimsWhenStale: ViewModifier {
    let host: HostModel

    func body(content: Content) -> some View {
        TimelineView(.explicit(host.reachabilityMarks)) { _ in
            content.opacity(host.staleAsOf(at: .now) == nil ? 1 : 0.45)
        }
    }
}

import SwiftUI

/// The floating glass pill of tabs at the foot of the screen: each tab a Lucide icon at 1.75 stroke over its name, the
/// selected one on a fill pill and in the tint colour. A count badge or a dot sits in ink on the icon's corner.
@MainActor
public struct GlassTabBar<ID: Hashable>: View {
    public struct Item: Identifiable {
        public let id: ID
        public let title: String
        public let icon: Lucide
        public let badge: Int
        public let dot: Bool

        public init(_ id: ID, _ title: String, icon: Lucide, badge: Int = 0, dot: Bool = false) {
            self.id = id
            self.title = title
            self.icon = icon
            self.badge = badge
            self.dot = dot
        }
    }

    let items: [Item]
    @Binding var selection: ID

    public init(_ items: [Item], selection: Binding<ID>) {
        self.items = items
        _selection = selection
    }

    /// The bar's height over the home indicator, for content to scroll clear of it.
    public static var clearance: CGFloat { 62 + 29 }

    public var body: some View {
        HStack(spacing: 0) {
            ForEach(items) { item in
                Button {
                    Haptics.tap()
                    selection = item.id
                } label: {
                    tab(item, selected: item.id == selection)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(item.badge > 0 ? "\(item.title), \(item.badge)" : item.title)
                .accessibilityAddTraits(item.id == selection ? .isSelected : [])
            }
        }
        .padding(.horizontal, 4)
        .frame(height: 62)
        .background(Palette.glass.color, in: Capsule())
        .glassBackdrop()
        .overlay(Capsule().strokeBorder(Palette.glassBorder.color, lineWidth: 0.5))
        .shadow(color: .black.opacity(0.22), radius: 15, y: 8)
        .padding(.horizontal, 21)
    }

    private func tab(_ item: Item, selected: Bool) -> some View {
        VStack(spacing: 2) {
            LucideIcon(item.icon, size: 24, weight: 1.75)
                .overlay(alignment: .topLeading) {
                    if item.badge > 0 {
                        Text("\(item.badge)")
                            .font(.system(size: 12, weight: .semibold))
                            .monospacedDigit()
                            .foregroundStyle(Palette.onText.color)
                            .padding(.horizontal, 5)
                            .frame(minWidth: 18, minHeight: 18)
                            .background(Palette.text.color, in: Capsule())
                            .offset(x: 14, y: -5)
                    } else if item.dot {
                        Circle().fill(Palette.text.color).frame(width: 9, height: 9)
                            .overlay(Circle().stroke(Palette.surface.color, lineWidth: 2))
                            .offset(x: 18, y: -2)
                    }
                }
            Text(item.title).font(.system(size: 10, weight: .semibold)).tracking(0.1).lineLimit(1)
        }
        .foregroundStyle((selected ? Palette.accent : Palette.text2).color)
        .frame(maxWidth: .infinity)
        .frame(height: 54)
        .background(selected ? Palette.fill3.color : .clear, in: Capsule())
        .contentShape(Capsule())
    }
}

extension View {
    /// Blurs what scrolls under a glass surface; the tint over it is the caller's.
    fileprivate func glassBackdrop() -> some View {
        background(.ultraThinMaterial, in: Capsule())
    }
}

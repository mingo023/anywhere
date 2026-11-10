import SwiftUI

/// One line of the iPad sidebar: an icon, a title that keeps its width over `detail`, and a fixed trailing slot.
/// `detail` shows whole or not at all.
@MainActor
public struct SidebarRow<Icon: View, Trailing: View>: View {
    public static var height: CGFloat { 32 }

    let title: String
    let detail: String?
    let strong: Bool
    let selected: Bool
    @ViewBuilder let icon: Icon
    @ViewBuilder let trailing: Trailing

    public init(
        _ title: String, detail: String? = nil, strong: Bool = false, selected: Bool, @ViewBuilder icon: () -> Icon,
        @ViewBuilder trailing: () -> Trailing
    ) {
        self.title = title
        self.detail = detail
        self.strong = strong
        self.selected = selected
        self.icon = icon()
        self.trailing = trailing()
    }

    public var body: some View {
        HStack(spacing: Space.s) {
            icon.font(.system(size: 16)).foregroundStyle(selected ? Palette.text.color : Palette.text3.color).frame(minWidth: 16, minHeight: 16)
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 6) {
                    name.fixedSize()
                    if let detail {
                        Text(detail).font(.system(.caption, design: .monospaced)).foregroundStyle(Palette.text3.color).fixedSize()
                    }
                }
                name
            }
            Spacer(minLength: 0)
            trailing.frame(minWidth: 16, minHeight: 16)
        }
        .frame(height: Self.height)
    }

    private var name: some View {
        Text(title).font(strong || selected ? Typography.subheadline.weight(.semibold) : Typography.subheadline)
            .foregroundStyle(Palette.text.color).lineLimit(1).truncationMode(.tail)
    }
}

/// What folds a sidebar row, pointing down while it's open.
@MainActor
public struct SidebarChevron: View {
    let open: Bool

    public init(open: Bool) { self.open = open }

    public var body: some View {
        Image(systemName: open ? "chevron.down" : "chevron.right")
            .font(.system(size: 10, weight: .semibold)).foregroundStyle(Palette.text4.color).frame(width: 16, height: 16)
    }
}

/// A count in the ink tint, for what wants a look.
@MainActor
public struct SidebarCount: View {
    let count: Int

    public init(_ count: Int) { self.count = count }

    public var body: some View {
        Text("\(count)").font(Typography.digits.weight(.semibold)).foregroundStyle(Palette.text.color)
            .padding(.horizontal, 7).frame(height: 20).background(Palette.accentBackground.color, in: Capsule())
    }
}

public extension View {
    /// Places a `SidebarRow` in its list, `indent` from the column's edge, with the inset selection fill. `gap` opens space
    /// above it, outside the fill.
    func sidebarListRow(selected: Bool, indent: CGFloat, gap: CGFloat = 0) -> some View {
        listRowInsets(EdgeInsets(top: gap, leading: indent, bottom: 0, trailing: 20))
            .listRowBackground(SidebarSelection(selected: selected).padding(.top, gap))
    }
}

/// The inset fill behind the selected sidebar row.
@MainActor
public struct SidebarSelection: View {
    let selected: Bool

    public init(selected: Bool) { self.selected = selected }

    public var body: some View {
        RoundedRectangle(cornerRadius: Radius.small, style: .continuous)
            .fill(selected ? Palette.rowSelected.color : .clear)
            .padding(.horizontal, 10)
    }
}

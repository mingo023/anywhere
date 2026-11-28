import DesignSystem
import SwiftUI

/// A file's bar over its diff: chevron, path and counts at the start, then its menu and whether it's been viewed. A
/// caller lays out `FileHeaderLead` and `FileHeaderTail` in `fileHeaderBar()`, so the lead can be a link and the tail
/// keep its own taps.
@MainActor
struct FileHeaderLead: View {
    let title: String
    let open: Bool
    let added: Int?
    let removed: Int?

    var body: some View {
        HStack(spacing: Space.s) {
            LucideIcon(open ? .chevronDown : .chevronRight, size: 15).foregroundStyle(Palette.text2.color).frame(width: 16)
            Text(title)
                .font(.system(size: 13, design: .monospaced))
                .foregroundStyle(Palette.text.color)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: Space.xs)
            if let added, let removed {
                HStack(spacing: Space.s) {
                    Text("+\(added)").foregroundStyle(Palette.successText.color)
                    Text("−\(removed)").foregroundStyle(Palette.failedText.color)
                }
                .font(.system(size: 12, design: .monospaced))
            }
        }
        .frame(maxHeight: .infinity)
        .contentShape(Rectangle())
    }
}

@MainActor
struct FileHeaderTail<MenuContent: View>: View {
    let viewed: Bool
    let toggleViewed: () -> Void
    @ViewBuilder let menu: MenuContent

    var body: some View {
        HStack(spacing: Space.xs) {
            Menu { menu } label: {
                LucideIcon(.ellipsis, size: 16).foregroundStyle(Palette.text2.color).frame(width: 32, height: HitTarget.minimum)
            }
            Button(action: toggleViewed) { CheckCircle(on: viewed).frame(width: 32, height: HitTarget.minimum) }
                .buttonStyle(.plain)
                .accessibilityLabel(viewed ? "Viewed" : "Mark as Viewed")
        }
    }
}

extension View {
    /// The bar a file's header sits in: 44 pt on the surface, between hairlines, full width.
    func fileHeaderBar() -> some View {
        padding(.leading, 10)
            .padding(.trailing, Space.xs)
            .frame(height: HitTarget.minimum)
            .frame(maxWidth: .infinity)
            .background(Palette.surface.color)
            .overlay(alignment: .top) { Palette.separator.color.frame(height: 0.5) }
            .overlay(alignment: .bottom) { Palette.separator.color.frame(height: 0.5) }
    }
}

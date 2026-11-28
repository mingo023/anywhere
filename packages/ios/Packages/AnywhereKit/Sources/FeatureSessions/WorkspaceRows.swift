import DesignSystem
import Domain
import SwiftUI

/// A project's initials on a tile, with the desktop's corner badges: a dot for what wants a look, a spinner for work. The
/// iPad sidebar's.
@MainActor
struct ProjectAvatar: View {
    let name: String
    let statuses: [Status]
    var side = ProjectAvatar.size

    nonisolated static let size: CGFloat = 40

    var body: some View {
        RoundedRectangle(cornerRadius: side / 4, style: .continuous)
            .fill(Palette.fill3.color)
            .overlay(RoundedRectangle(cornerRadius: side / 4, style: .continuous).strokeBorder(Palette.separator.color))
            .overlay(Text(Workspaces.initials(name)).font(side < Self.size ? .system(size: 9, weight: .semibold, design: .monospaced) : Typography.captionStrong).foregroundStyle(Palette.text2.color))
            .frame(width: side, height: side)
            .overlay(alignment: .topTrailing) {
                if let alert = Workspaces.alert(statuses) {
                    Circle().fill(StatusGlyph.color(alert).color).frame(width: 9, height: 9)
                        .padding(2).background(Palette.background.color, in: Circle())
                        .offset(x: 4, y: -4)
                }
            }
            .overlay(alignment: .bottomTrailing) {
                if statuses.contains(.working) {
                    GradientSpinner(size: 10).frame(width: 12, height: 12).background(Palette.background.color, in: Circle())
                        .offset(x: 3, y: 3)
                }
            }
            .accessibilityHidden(true)
    }
}

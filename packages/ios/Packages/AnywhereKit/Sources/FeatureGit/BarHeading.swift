import DesignSystem
import SwiftUI

/// A bar's centred title with a quieter mono line under it: "Changes" over the branch, "Commit" over the sha.
@MainActor
struct BarHeading<Detail: View>: View {
    let title: String
    let detail: Detail?

    init(_ title: String, @ViewBuilder detail: () -> Detail) {
        self.title = title
        self.detail = detail()
    }

    var body: some View {
        VStack(spacing: 0) {
            Text(title).font(.system(size: 17, weight: .semibold)).foregroundStyle(Palette.text.color)
            if let detail {
                detail.font(.system(size: 12, design: .monospaced)).foregroundStyle(Palette.text2.color)
            }
        }
        .lineLimit(1)
        .truncationMode(.middle)
    }
}

extension BarHeading where Detail == Text {
    init(_ title: String, detail: String?) {
        self.title = title
        self.detail = detail.map { Text($0) }
    }
}

/// The round "…" of a bar's corner, for a menu.
@MainActor
struct BarMenuLabel: View {
    var body: some View {
        Label { Text("Options") } icon: { Image.lucide(.ellipsis, size: 20) }
    }
}

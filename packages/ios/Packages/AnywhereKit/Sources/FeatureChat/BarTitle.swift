import DesignSystem
import SwiftUI

extension View {
    /// A title over a small status line, centred in the bar on every OS, as a messenger names a conversation. iOS 26's
    /// navigation subtitle sets the title at the leading edge beside Back instead.
    func barTitle(_ title: String, subtitle: String) -> some View {
        navigationTitle(title).toolbar {
            ToolbarItem(placement: .principal) { BarTitle(title: title, subtitle: subtitle) }
        }
    }
}

@MainActor
private struct BarTitle: View {
    let title: String
    let subtitle: String

    var body: some View {
        VStack(spacing: 1) {
            Text(title).font(.system(size: 16, weight: .semibold)).foregroundStyle(Palette.text.color)
            Text(subtitle).font(.system(size: 12)).foregroundStyle(Palette.text3.color)
        }
        .lineLimit(1)
        .accessibilityElement(children: .combine)
    }
}

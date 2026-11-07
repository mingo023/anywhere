import DesignSystem
import SwiftUI

extension View {
    /// The file's name over its folder · language · size, as one two-line title.
    @ViewBuilder
    func fileTitle(_ name: String, subtitle: String?) -> some View {
        #if os(iOS)
            if #available(iOS 26, *) {
                navigationTitle(name).navigationSubtitle(subtitle ?? "")
            } else {
                navigationTitle(name).toolbar {
                    ToolbarItem(placement: .principal) { FileTitle(name: name, subtitle: subtitle) }
                }
            }
        #else
            navigationTitle(name).navigationSubtitle(subtitle ?? "")
        #endif
    }
}

/// The two-line title before iOS 26, which has no navigation subtitle.
@MainActor
private struct FileTitle: View {
    let name: String
    let subtitle: String?

    var body: some View {
        VStack(spacing: 0) {
            Text(name).font(Typography.headline).foregroundStyle(Palette.text.color)
            if let subtitle {
                Text(subtitle).font(Typography.caption).foregroundStyle(Palette.text3.color)
            }
        }
        .lineLimit(1)
        .truncationMode(.middle)
        .accessibilityElement(children: .combine)
    }
}

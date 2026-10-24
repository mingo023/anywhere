import DesignSystem
import MarkdownModel
import SwiftUI
#if os(iOS)
    import UIKit
#else
    import AppKit
#endif

/// The bytes of a `data:` URL's image, decoded.
nonisolated func dataURLBytes(_ source: String) -> Data? {
    guard source.hasPrefix("data:"), let comma = source.firstIndex(of: ",") else { return nil }
    let meta = source[source.index(source.startIndex, offsetBy: 5)..<comma]
    let payload = String(source[source.index(after: comma)...])
    if meta.hasSuffix(";base64") { return Data(base64Encoded: payload, options: .ignoreUnknownCharacters) }
    return payload.removingPercentEncoding.map { Data($0.utf8) }
}

/// An image and the link around it, if any.
struct LinkedImage: Equatable {
    let image: MDImage
    let link: String?
}

/// Images side by side, wrapping onto more rows as they need, like a README's badges.
@MainActor
struct ImageRow: View {
    let images: [LinkedImage]
    let height: CGFloat

    var body: some View {
        FlowLayout(spacing: 6) {
            ForEach(images.indices, id: \.self) { i in MarkdownImage(image: images[i].image, link: images[i].link) }
        }
    }
}

/// One image: fetched for http(s), decoded for `data:`, else its alt text in a chip.
@MainActor
struct MarkdownImage: View {
    let image: MDImage
    let link: String?
    @Environment(\.openURL) private var openURL

    var body: some View {
        content
            .accessibilityLabel(image.alt.isEmpty ? "Image" : image.alt)
            .onTapGesture {
                if let link, let url = LinkURL.url(for: link) { openURL(url) }
            }
    }

    @ViewBuilder
    private var content: some View {
        if let data = dataURLBytes(image.source), let platform = PlatformImage(data: data) {
            sized(Image(platformImage: platform).resizable(), aspect: platform.size.width / max(platform.size.height, 1))
        } else if let url = URL(string: image.source), url.scheme == "http" || url.scheme == "https" {
            AsyncImage(url: url, transaction: Transaction(animation: .easeOut(duration: 0.2))) { phase in
                switch phase {
                case .success(let loaded): sized(loaded.resizable(), aspect: nil)
                case .failure: placeholder("photo.badge.exclamationmark")
                default:
                    RoundedRectangle(cornerRadius: 6).fill(Palette.fill2.color)
                        .frame(width: image.width ?? 160, height: image.height ?? 90)
                }
            }
        } else {
            placeholder("photo")
        }
    }

    private func sized(_ view: Image, aspect: CGFloat?) -> some View {
        Group {
            if let w = image.width, let h = image.height {
                view.aspectRatio(contentMode: .fit).frame(width: w, height: h)
            } else if let w = image.width {
                view.aspectRatio(aspect, contentMode: .fit).frame(width: w)
            } else if let h = image.height {
                view.aspectRatio(aspect, contentMode: .fit).frame(height: h)
            } else {
                view.aspectRatio(aspect, contentMode: .fit).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
    }

    private func placeholder(_ symbol: String) -> some View {
        Label(image.alt.isEmpty ? image.source : image.alt, systemImage: symbol)
            .font(.system(size: 12))
            .foregroundStyle(Palette.text2.color)
            .lineLimit(1)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: 6))
    }
}

#if os(iOS)
    typealias PlatformImage = UIImage
    extension Image {
        init(platformImage: UIImage) { self.init(uiImage: platformImage) }
    }
#else
    typealias PlatformImage = NSImage
    extension Image {
        init(platformImage: NSImage) { self.init(nsImage: platformImage) }
    }
#endif

/// Views left to right, wrapping when the row is full.
struct FlowLayout: Layout {
    var spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(proposal.width ?? .infinity, subviews)
        return CGSize(width: rows.map(\.width).max() ?? 0, height: rows.map(\.height).reduce(0, +) + spacing * CGFloat(max(0, rows.count - 1)))
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in arrange(bounds.width, subviews) {
            var x = bounds.minX
            for i in row.items {
                let size = subviews[i].sizeThatFits(ProposedViewSize(width: bounds.width, height: nil))
                subviews[i].place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
                x += size.width + spacing
            }
            y += row.height + spacing
        }
    }

    private func arrange(_ width: CGFloat, _ subviews: Subviews) -> [(items: [Int], width: CGFloat, height: CGFloat)] {
        var rows: [(items: [Int], width: CGFloat, height: CGFloat)] = []
        var current: (items: [Int], width: CGFloat, height: CGFloat) = ([], 0, 0)
        for (i, view) in subviews.enumerated() {
            let size = view.sizeThatFits(ProposedViewSize(width: width, height: nil))
            if !current.items.isEmpty, current.width + spacing + size.width > width {
                rows.append(current)
                current = ([], 0, 0)
            }
            current.width += (current.items.isEmpty ? 0 : spacing) + size.width
            current.height = max(current.height, size.height)
            current.items.append(i)
        }
        if !current.items.isEmpty { rows.append(current) }
        return rows
    }
}

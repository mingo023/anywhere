import AppKit
import SwiftUI

struct ProviderIcon: View {
    let ask: Ask
    let size: CGFloat
    private static var images: [String: NSImage] = [:]

    var body: some View {
        Group {
            if let image = Self.image(ask.icon) {
                Image(nsImage: image).resizable().renderingMode(.template)
            } else {
                Image(systemName: "terminal").resizable()
            }
        }
        .aspectRatio(contentMode: .fit)
        .frame(width: size, height: size)
        .foregroundColor(ask.provider == "claude" ? .claude : .ink)
    }

    private static func image(_ svg: String) -> NSImage? {
        if let image = images[svg] {
            return image
        }
        let image = NSImage(data: Data(svg.utf8))
        image?.isTemplate = true
        images[svg] = image
        return image
    }
}

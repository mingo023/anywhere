import CoreGraphics
import SwiftUI

/// A Lucide icon drawn as the desktop draws it: outline only, round caps and joins, `weight` units of stroke on the
/// 24-unit grid (2, or 1.75 in the tab bar), tinted with the foreground style.
@MainActor
public struct LucideIcon: View {
    let icon: Lucide
    let size: CGFloat
    let weight: CGFloat

    public init(_ icon: Lucide, size: CGFloat = 17, weight: CGFloat = 2) {
        self.icon = icon
        self.size = size
        self.weight = weight
    }

    public var body: some View {
        icon.outline
            .applying(CGAffineTransform(scaleX: size / 24, y: size / 24))
            .stroke(style: StrokeStyle(lineWidth: weight * size / 24, lineCap: .round, lineJoin: .round))
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

extension Lucide {
    @MainActor private static var outlines: [Lucide: Path] = [:]
    @MainActor private static var drawn: [String: CGImage] = [:]

    /// `path`, parsed once.
    @MainActor var outline: Path {
        if let cached = Self.outlines[self] { return cached }
        let parsed = path
        Self.outlines[self] = parsed
        return parsed
    }

    /// The icon as a template image, for the places that take only an `Image`: `Label`, menus, swipe actions.
    @MainActor
    func image(size: CGFloat, weight: CGFloat) -> Image {
        let scale: CGFloat = 3
        let key = "\(rawValue)-\(size)-\(weight)"
        if Self.drawn[key] == nil { Self.drawn[key] = draw(pixels: Int((size * scale).rounded()), weight: weight) }
        guard let image = Self.drawn[key] else { return Image(systemName: "questionmark") }
        return Image(decorative: image, scale: scale).renderingMode(.template)
    }

    @MainActor
    private func draw(pixels: Int, weight: CGFloat) -> CGImage? {
        guard let context = CGContext(
            data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        let unit = CGFloat(pixels) / 24
        context.translateBy(x: 0, y: CGFloat(pixels))
        context.scaleBy(x: unit, y: -unit)
        context.setStrokeColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
        context.setLineWidth(weight)
        context.setLineCap(.round)
        context.setLineJoin(.round)
        context.addPath(outline.cgPath)
        context.strokePath()
        return context.makeImage()
    }
}

extension Image {
    /// A Lucide icon as an image `size` points square.
    @MainActor
    public static func lucide(_ icon: Lucide, size: CGFloat = 17, weight: CGFloat = 2) -> Image {
        icon.image(size: size, weight: weight)
    }
}

extension Label where Title == Text, Icon == Image {
    /// A label with a Lucide icon, as `Label(_:systemImage:)`.
    @MainActor
    public init(_ title: some StringProtocol, lucide icon: Lucide, size: CGFloat = 17) {
        self.init { Text(title) } icon: { Image.lucide(icon, size: size) }
    }
}

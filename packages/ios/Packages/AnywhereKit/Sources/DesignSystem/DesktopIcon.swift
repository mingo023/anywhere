import CoreGraphics
import SwiftUI

extension Image {
    /// An icon by name: the desktop's for "worktree", "branch" and "fullscreen" (`crates/theme/assets/icons`), which SF
    /// Symbols has no match for, else that SF Symbol. `size` is in points and applies only to the desktop's, which don't follow the font.
    @MainActor
    public static func named(_ name: String, size: CGFloat) -> Image {
        guard let glyph = DesktopIcon(rawValue: name), let image = glyph.image(size: size) else { return Image(systemName: name) }
        return image
    }
}

/// Lucide outlines from the desktop's SVGs, on their 24-unit grid, drawn once per size since iOS can't draw an SVG file.
private enum DesktopIcon: String {
    case worktree, branch, fullscreen

    @MainActor private static var drawn: [String: CGImage] = [:]

    @MainActor
    func image(size: CGFloat) -> Image? {
        let key = "\(rawValue)-\(size)"
        let scale: CGFloat = 3
        if Self.drawn[key] == nil { Self.drawn[key] = draw(pixels: Int(size * scale)) }
        return Self.drawn[key].map { Image(decorative: $0, scale: scale).renderingMode(.template) }
    }

    private func draw(pixels: Int) -> CGImage? {
        guard let context = CGContext(
            data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        let unit = CGFloat(pixels) / 24
        context.translateBy(x: 0, y: CGFloat(pixels))
        context.scaleBy(x: unit, y: -unit)
        context.setStrokeColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
        context.setLineWidth(self == .fullscreen ? 1.6 : 2)
        context.setLineCap(.round)
        context.setLineJoin(.round)
        context.addPath(outline.cgPath)
        context.strokePath()
        return context.makeImage()
    }

    private var outline: Path {
        var path = Path()
        func ring(_ x: CGFloat, _ y: CGFloat) { path.addEllipse(in: CGRect(x: x - 3, y: y - 3, width: 6, height: 6)) }
        switch self {
        case .worktree:
            ring(12, 18)
            ring(6, 6)
            ring(18, 6)
            path.move(to: CGPoint(x: 18, y: 9))
            path.addLine(to: CGPoint(x: 18, y: 11))
            path.addCurve(to: CGPoint(x: 17, y: 12), control1: CGPoint(x: 18, y: 11.6), control2: CGPoint(x: 17.6, y: 12))
            path.addLine(to: CGPoint(x: 7, y: 12))
            path.addCurve(to: CGPoint(x: 6, y: 11), control1: CGPoint(x: 6.4, y: 12), control2: CGPoint(x: 6, y: 11.6))
            path.addLine(to: CGPoint(x: 6, y: 9))
            path.move(to: CGPoint(x: 12, y: 12))
            path.addLine(to: CGPoint(x: 12, y: 15))
        case .branch:
            ring(18, 6)
            ring(6, 18)
            path.move(to: CGPoint(x: 15, y: 6))
            path.addCurve(to: CGPoint(x: 6, y: 15), control1: CGPoint(x: 10.03, y: 6), control2: CGPoint(x: 6, y: 10.03))
            path.move(to: CGPoint(x: 6, y: 15))
            path.addLine(to: CGPoint(x: 6, y: 3))
        case .fullscreen:
            let brackets: [(CGPoint, CGPoint, CGPoint)] = [
                (CGPoint(x: 4, y: 8), CGPoint(x: 4, y: 4), CGPoint(x: 8, y: 4)),
                (CGPoint(x: 16, y: 4), CGPoint(x: 20, y: 4), CGPoint(x: 20, y: 8)),
                (CGPoint(x: 20, y: 16), CGPoint(x: 20, y: 20), CGPoint(x: 16, y: 20)),
                (CGPoint(x: 8, y: 20), CGPoint(x: 4, y: 20), CGPoint(x: 4, y: 16)),
            ]
            for (start, corner, end) in brackets {
                path.move(to: start)
                path.addArc(tangent1End: corner, tangent2End: end, radius: 2)
                path.addLine(to: end)
            }
        }
        return path
    }
}

import SwiftUI

/// A colour with a value per appearance, `0xRRGGBBAA` each, as the desktop's `theme::Token`.
public struct Token: Hashable, Sendable {
    public let light: UInt32
    public let dark: UInt32

    public init(_ light: UInt32, _ dark: UInt32) {
        self.light = light
        self.dark = dark
    }

    public static func fixed(_ rgba: UInt32) -> Token { Token(rgba, rgba) }

    public func rgba(dark isDark: Bool) -> UInt32 { isDark ? dark : light }

    /// Red, green, blue and alpha in 0...1.
    public static func components(_ rgba: UInt32) -> (Double, Double, Double, Double) {
        (Double(rgba >> 24 & 0xff) / 255, Double(rgba >> 16 & 0xff) / 255, Double(rgba >> 8 & 0xff) / 255, Double(rgba & 0xff) / 255)
    }

    /// Resolves per appearance when drawn.
    public var color: Color {
        #if os(iOS)
            Color(uiColor: UIColor { traits in
                let (r, g, b, a) = Token.components(rgba(dark: traits.userInterfaceStyle == .dark))
                return UIColor(red: r, green: g, blue: b, alpha: a)
            })
        #else
            Color(nsColor: NSColor(name: nil) { appearance in
                let dark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                let (r, g, b, a) = Token.components(rgba(dark: dark))
                return NSColor(srgbRed: r, green: g, blue: b, alpha: a)
            })
        #endif
    }
}

extension ShapeStyle where Self == Color {
    public static func token(_ t: Token) -> Color { t.color }
}

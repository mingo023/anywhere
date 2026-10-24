import SwiftUI

/// SF Pro for text and SF Mono for code, on Dynamic Type's text styles so they follow the reader's size.
public enum Typography {
    public static let largeTitle = Font.system(.largeTitle, weight: .bold)
    public static let title = Font.system(.title2, weight: .semibold)
    public static let headline = Font.system(.headline)
    public static let body = Font.system(.body)
    public static let callout = Font.system(.callout)
    public static let subheadline = Font.system(.subheadline)
    public static let footnote = Font.system(.footnote)
    public static let caption = Font.system(.caption)
    public static let captionStrong = Font.system(.caption, weight: .semibold)

    public static let code = Font.system(.callout, design: .monospaced)
    public static let codeSmall = Font.system(.footnote, design: .monospaced)
    /// Elapsed times and counts that tick, so their width holds still.
    public static let digits = Font.system(.footnote).monospacedDigit()
}

/// The spacing scale, in points.
public enum Space {
    public static let xxs: CGFloat = 2
    public static let xs: CGFloat = 4
    public static let s: CGFloat = 8
    public static let m: CGFloat = 12
    public static let l: CGFloat = 16
    public static let xl: CGFloat = 24
    public static let xxl: CGFloat = 32
}

public enum Radius {
    public static let chip: CGFloat = 5
    public static let small: CGFloat = 8
    public static let card: CGFloat = 14
    public static let sheet: CGFloat = 22
}

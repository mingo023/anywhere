import SwiftUI

/// The smallest tap area Apple's guidelines allow, in points.
public enum HitTarget {
    public static let minimum: CGFloat = 44
}

extension View {
    /// Grows a small control's tappable area to 44×44 around its centre, so a glyph or pill drawn smaller is still easy
    /// to hit. The control takes that room in its layout. On a `Button`, apply it to the label: outside the button the
    /// extra area has no tap to hand on.
    public func hitTarget() -> some View {
        frame(minWidth: HitTarget.minimum, minHeight: HitTarget.minimum).contentShape(Rectangle())
    }
}

import SwiftUI

extension View {
    /// Liquid Glass where the OS has it (iOS 26), a material otherwise.
    @ViewBuilder
    public func glass<S: Shape>(in shape: S) -> some View {
        #if os(iOS) && compiler(>=6.2)
            if #available(iOS 26, *) {
                glassEffect(.regular, in: shape)
            } else {
                background(.regularMaterial, in: shape)
            }
        #else
            background(.regularMaterial, in: shape)
        #endif
    }

    /// A solid card floating over content, as toasts, banners and bottom bars sit: a hairline edge and a soft shadow.
    public func raised<S: InsettableShape>(in shape: S, fill: Token = Palette.popover) -> some View {
        background(fill.color, in: shape)
            .overlay(shape.strokeBorder(Palette.hairline.color, lineWidth: 0.5))
            .shadow(color: Palette.shadow.color, radius: 16, y: 4)
    }

    /// A solid band behind the navigation bar, so content scrolling under it never meets the bar's words; it fades out over
    /// `fade` points below the bar.
    public func solidBar(_ color: Color, fade: CGFloat = 0) -> some View {
        overlay(alignment: .top) {
            VStack(spacing: 0) {
                Color.clear.frame(height: 0).background(color, ignoresSafeAreaEdges: .top)
                LinearGradient(colors: [color, color.opacity(0)], startPoint: .top, endPoint: .bottom).frame(height: fade)
            }
            .allowsHitTesting(false)
        }
    }
}

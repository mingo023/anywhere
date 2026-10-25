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
}

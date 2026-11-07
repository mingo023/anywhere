import SwiftUI

extension View {
    /// The chat's bar as a translucent blur the transcript scrolls under, with no line beneath it. iOS 26 gives that
    /// itself with glass and its soft scroll edge; before it, a bar material stands in for the bar's own background,
    /// which draws a hairline under it once content scrolls beneath.
    @ViewBuilder
    func chatBar() -> some View {
        #if os(iOS)
            if #available(iOS 26, *) {
                self
            } else {
                toolbarBackground(.hidden, for: .navigationBar)
                    .overlay(alignment: .top) { Color.clear.frame(height: 0).background(.bar, ignoresSafeAreaEdges: .top) }
            }
        #else
            self
        #endif
    }
}

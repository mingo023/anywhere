import DesignSystem
import SwiftUI

extension View {
    /// The chat's bar with no line beneath it. On iOS 26 a solid band keeps the transcript off the title, which the soft
    /// scroll edge lets through; before it, a bar material stands in for the bar's own background, which draws a hairline
    /// under it once content scrolls beneath.
    @ViewBuilder
    func chatBar() -> some View {
        #if os(iOS)
            if #available(iOS 26, *) {
                solidBar(Palette.background.color, fade: 20)
            } else {
                toolbarBackground(.hidden, for: .navigationBar)
                    .overlay(alignment: .top) { Color.clear.frame(height: 0).background(.bar, ignoresSafeAreaEdges: .top) }
            }
        #else
            self
        #endif
    }
}

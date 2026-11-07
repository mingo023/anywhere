import DesignSystem
import SwiftUI

/// A knob on a pick's top or bottom edge; dragging it moves that edge a row per row height.
@MainActor
struct PickHandle: View {
    let edge: LinePick.Edge
    let rowHeight: CGFloat
    /// Rows moved since the drag began, as the finger moves.
    let moved: (Int) -> Void
    let ended: () -> Void

    var body: some View {
        Circle()
            .fill(Palette.link.color)
            .frame(width: 12, height: 12)
            .frame(width: 44, height: 44)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { moved(Int(($0.translation.height / rowHeight).rounded())) }
                    .onEnded { _ in ended() }
            )
            .accessibilityHidden(true)
    }
}

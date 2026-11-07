import SwiftUI

/// A pick's top and bottom handles, each drawn by the block of rows that holds it.
@MainActor
struct PickHandles: View {
    let handles: (top: Int, bottom: Int)
    /// The block's rows.
    let range: Range<Int>
    /// Where the code starts across the row.
    let x: CGFloat
    let rowHeight: CGFloat
    let moved: (LinePick.Edge, Int) -> Void
    let ended: () -> Void

    var body: some View {
        if range.contains(handles.top) {
            PickHandle(edge: .top, rowHeight: rowHeight, moved: { moved(.top, $0) }, ended: ended)
                .offset(x: x - 22, y: y(handles.top) - 22)
        }
        if range.contains(handles.bottom) {
            PickHandle(edge: .bottom, rowHeight: rowHeight, moved: { moved(.bottom, $0) }, ended: ended)
                .offset(x: x - 22, y: y(handles.bottom) + rowHeight - 22)
        }
    }

    private func y(_ row: Int) -> CGFloat { CGFloat(row - range.lowerBound) * rowHeight }
}

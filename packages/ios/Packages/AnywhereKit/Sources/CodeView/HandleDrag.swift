/// The pick's handle being dragged, if any, and where both handles sit on a view's rows.
struct HandleDrag {
    /// The handle taken hold of and the row it started from.
    private var held: (edge: LinePick.Edge, from: Int)?

    /// The rows the pick's handles sit on: the top one's first row, the bottom one's last. A handle being dragged
    /// stays on the row it started from and follows the finger by offset; the other stays on the anchor.
    func rows(_ pick: LinePick, in rows: CodeRows) -> (top: Int, bottom: Int) {
        let top = held.map { $0.edge == .top ? $0.from : rows.first(of: pick.anchor) } ?? rows.first(of: pick.lines.lowerBound)
        let bottom = held.map { $0.edge == .bottom ? $0.from : rows.rows(of: pick.anchor).upperBound - 1 }
            ?? rows.rows(of: pick.lines.upperBound).upperBound - 1
        return (top, bottom)
    }

    /// Moves `edge`'s handle `by` rows from where it was taken hold of, stretching `pick` to that row's line if `allowed`.
    mutating func move(
        _ edge: LinePick.Edge, by: Int, pick: inout LinePick?, rows: CodeRows, allowed: (Int, Int) -> Bool = { _, _ in true }
    ) {
        guard let picked = pick?.lines else { return }
        let from = held?.from ?? (edge == .top ? rows.first(of: picked.lowerBound) : rows.rows(of: picked.upperBound).upperBound - 1)
        if held == nil {
            held = (edge, from)
            pick?.grab(edge)
        }
        let row = (from + by).clamped(to: 0...max(rows.count - 1, 0))
        _ = pick?.drag(to: rows[row].line, allowed: allowed)
    }

    mutating func end() { held = nil }
}

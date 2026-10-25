import DesignSystem
import MarkdownModel
import SwiftUI

/// Column widths for a table: each column's natural width, grown to fill `available`, or shrunk by wrapping toward
/// its floor (about two lines, kept within 160–480) when the natural widths don't fit. Past the floors the table is
/// wider than `available` and scrolls (desktop `pocket/chat/markdown.rs style()` table, `node.rs` column sizing).
nonisolated func tableColumnWidths(natural: [CGFloat], available: CGFloat, floorRange: ClosedRange<CGFloat> = 160...480) -> [CGFloat] {
    guard !natural.isEmpty else { return [] }
    let total = natural.reduce(0, +)
    if total <= available {
        let extra = (available - total) / CGFloat(natural.count)
        return natural.map { $0 + extra }
    }
    let floors = natural.map { min($0, min(max($0 / 2, floorRange.lowerBound), floorRange.upperBound)) }
    let give = zip(natural, floors).map { $0 - $1 }
    let room = give.reduce(0, +)
    let need = total - available
    guard room > 0 else { return natural }
    let share = min(1, need / room)
    return zip(natural, give).map { $0 - $1 * share }
}

/// A table: framed with a radius of 10, inner rules, cells padded 12, its head in the strong colour, columns aligned
/// as written, scrolling sideways when it can't fit.
@MainActor
struct TableView: View {
    let table: MDTable
    let metrics: Metrics
    @State private var available: CGFloat = 0

    var body: some View {
        let columns = max(table.alignments.count, table.header.count)
        ScrollView(.horizontal, showsIndicators: true) {
            TableLayout(columns: columns, available: available) {
                ForEach(Array(([table.header] + table.rows).enumerated()), id: \.offset) { r, row in
                    ForEach(0..<columns, id: \.self) { c in
                        cell(c < row.count ? row[c] : [], header: r == 0, column: c, lastColumn: c == columns - 1, lastRow: r == table.rows.count)
                    }
                }
            }
            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Palette.separatorStrong.color, lineWidth: 1))
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        }
        .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
        .background {
            GeometryReader { g in Color.clear.preference(key: WidthKey.self, value: g.size.width) }
        }
        .onPreferenceChange(WidthKey.self) { available = $0 }
    }

    func cell(_ inlines: [MDInline], header: Bool, column: Int, lastColumn: Bool, lastRow: Bool) -> some View {
        let alignment = column < table.alignments.count ? table.alignments[column] : .none
        let (frame, text): (Alignment, TextAlignment) = switch alignment {
        case .center: (.top, .center)
        case .right: (.topTrailing, .trailing)
        default: (.topLeading, .leading)
        }
        let style = InlineStyle(size: metrics.body, weight: header ? .semibold : .regular, color: header ? Palette.text : Palette.textBody)
        return InlineText.text(inlines, style: style)
            .codeChips(inlines)
            .lineSpacing(Metrics.spacing(size: metrics.body, line: metrics.line))
            .multilineTextAlignment(text)
            .selectableOnMac()
            .padding(12)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: frame)
            .overlay(alignment: .trailing) {
                if !lastColumn { Rectangle().fill(Palette.separator.color).frame(width: 1) }
            }
            .overlay(alignment: .bottom) {
                if !lastRow { Rectangle().fill(Palette.separator.color).frame(height: 1) }
            }
    }

    struct WidthKey: PreferenceKey {
        static let defaultValue: CGFloat = 0
        static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
    }
}

/// Cells in row order, laid out on shared column widths and row heights so their rules meet.
struct TableLayout: Layout {
    let columns: Int
    let available: CGFloat

    struct Cache {
        var sizes: [CGSize] = []
        var natural: [CGFloat] = []
        var widths: [CGFloat] = []
        var heights: [CGFloat] = []
        var available: CGFloat = -1
    }

    func makeCache(subviews: Subviews) -> Cache { Cache() }

    func updateCache(_ cache: inout Cache, subviews: Subviews) { cache = Cache() }

    func measure(_ subviews: Subviews, _ cache: inout Cache) {
        guard columns > 0, cache.available != available else { return }
        if cache.natural.isEmpty {
            cache.sizes = subviews.map { $0.sizeThatFits(.unspecified) }
            var natural = [CGFloat](repeating: 0, count: columns)
            for (i, size) in cache.sizes.enumerated() {
                natural[i % columns] = max(natural[i % columns], size.width.rounded(.up))
            }
            cache.natural = natural
        }
        let widths = tableColumnWidths(natural: cache.natural, available: max(available, 1))
        let rows = (subviews.count + columns - 1) / columns
        var heights = [CGFloat](repeating: 0, count: rows)
        for (i, view) in subviews.enumerated() {
            let width = widths[i % columns]
            // A cell given its natural width or more doesn't wrap, so its height is the one already measured.
            let height = width >= cache.sizes[i].width ? cache.sizes[i].height : view.sizeThatFits(ProposedViewSize(width: width, height: nil)).height
            heights[i / columns] = max(heights[i / columns], height)
        }
        cache.widths = widths
        cache.heights = heights
        cache.available = available
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) -> CGSize {
        measure(subviews, &cache)
        return CGSize(width: cache.widths.reduce(0, +), height: cache.heights.reduce(0, +))
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) {
        measure(subviews, &cache)
        var y = bounds.minY
        for row in cache.heights.indices {
            var x = bounds.minX
            for column in 0..<columns {
                let i = row * columns + column
                guard i < subviews.count else { break }
                subviews[i].place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(width: cache.widths[column], height: cache.heights[row]))
                x += cache.widths[column]
            }
            y += cache.heights[row]
        }
    }
}

import SwiftUI

/// Lays its children out left to right, wrapping to a new line when the next one doesn't fit.
struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    /// The children each line holds, by the widths they ask for.
    nonisolated static func lines(widths: [CGFloat], in available: CGFloat, spacing: CGFloat) -> [Range<Int>] {
        var lines: [Range<Int>] = []
        var start = 0
        var used: CGFloat = 0
        for (index, width) in widths.enumerated() {
            if index > start, used + spacing + width > available {
                lines.append(start..<index)
                start = index
                used = width
            } else {
                used += (index > start ? spacing : 0) + width
            }
        }
        if start < widths.count { lines.append(start..<widths.count) }
        return lines
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let sizes = subviews.map { $0.sizeThatFits(.unspecified) }
        var size = CGSize.zero
        for line in Self.lines(widths: sizes.map(\.width), in: proposal.width ?? .infinity, spacing: spacing) {
            let row = line.map { sizes[$0] }
            let width = row.reduce(spacing * CGFloat(row.count - 1)) { $0 + $1.width }
            size.width = max(size.width, width)
            size.height += (size.height > 0 ? spacing : 0) + (row.map(\.height).max() ?? 0)
        }
        return CGSize(width: proposal.width ?? size.width, height: size.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let sizes = subviews.map { $0.sizeThatFits(.unspecified) }
        var y = bounds.minY
        for line in Self.lines(widths: sizes.map(\.width), in: bounds.width, spacing: spacing) {
            var x = bounds.minX
            for index in line {
                subviews[index].place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(sizes[index]))
                x += sizes[index].width + spacing
            }
            y += (line.map { sizes[$0].height }.max() ?? 0) + spacing
        }
    }
}
